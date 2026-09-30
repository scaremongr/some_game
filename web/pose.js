// Камера и распознавание позы — плагин загрузчика miniquad.
//
// Игра не знает ни про камеру, ни про модель: она лишь спрашивает статус и
// забирает 13 точек скелета. Всё тяжёлое живёт здесь.
//
// Модель тянется с CDN и только при входе в режим. Класть её в наш бандл
// нельзя: это ещё несколько мегабайт к тем девяти, что уже есть, — а платит
// за них каждый, кто просто открыл игру.
(function () {
    var STATUS = {
        IDLE: 0,
        CAMERA: 1,   // просим доступ
        MODEL: 2,    // тянем модель
        RUNNING: 3,
        DENIED: 4,   // пользователь отказал
        UNSUPPORTED: 5,
        FAILED: 6,
    };

    // Индексы BlazePose (33 точки) в том же порядке, в каком их ждёт игра:
    // Head, ShoulderL/R, ElbowL/R, WristL/R, HipL/R, KneeL/R, AnkleL/R.
    var WANTED = [0, 11, 12, 13, 14, 15, 16, 23, 24, 25, 26, 27, 28];

    var MODEL_BASE = 'https://cdn.jsdelivr.net/npm/@mediapipe/tasks-vision@0.10.14';
    var MODEL_URL =
        'https://storage.googleapis.com/mediapipe-models/pose_landmarker/' +
        'pose_landmarker_lite/float16/1/pose_landmarker_lite.task';

    var status = STATUS.IDLE;
    // Настоящая причина отказа. Без неё на экране остаётся только «что-то не
    // завелось», а шагов тут четыре и падать может любой.
    var lastError = '';
    var video = null;
    var box = null;
    var overlay = null;
    var landmarker = null;
    // Float32Array(13*4): x, y, глубина, уверенность.
    var latest = null;
    var FLOATS_PER_JOINT = 4;
    var latestFresh = false;
    var videoAspect = 1.0;
    var lastVideoTime = -1;
    var failedFrames = 0;

    function describe(e) {
        if (!e) return 'unknown';
        var name = e.name || '';
        var message = e.message || String(e);
        return (name ? name + ': ' : '') + message;
    }

    /// Камеру не дали — показывать нечего, поток гасим.
    function failCamera(next, stage, e) {
        lastError = stage + ' / ' + describe(e);
        status = next;
        stopCamera();
    }

    /// Модель не завелась, но камера работает: оставляем её включённой.
    /// Оценки не будет, а зеркало и танцовщица на месте — это заметно лучше
    /// чёрного экрана.
    function failModel(stage, e) {
        lastError = stage + ' / ' + describe(e);
        status = STATUS.FAILED;
    }

    function stopCamera() {
        if (video && video.srcObject) {
            video.srcObject.getTracks().forEach(function (t) { t.stop(); });
            video.srcObject = null;
        }
        if (box) {
            box.style.display = 'none';
        }
    }

    // Окошко контроля в углу экрана: сама картинка с камеры и распознанный
    // скелет поверх неё. Играют не по нему — играют по аватару, — но без него
    // непонятно, видит тебя распознавание или нет.
    //
    // Всё это обычные элементы страницы поверх канваса. Тащить кадры в GL ради
    // такого незачем: браузер покажет их и быстрее, и без нашего участия.
    function ensureVideo() {
        if (video) return video;

        box = document.createElement('div');
        box.id = 'camera-box';
        box.style.cssText = [
            'position:fixed',
            'top:10px',
            'right:10px',
            // Доля экрана, но с потолком: на планшете окошко во весь угол
            // выглядело бы нелепо.
            'width:min(30vw,150px)',
            'aspect-ratio:4/3',
            'z-index:3',
            'border-radius:8px',
            'overflow:hidden',
            'border:1px solid rgba(138,160,255,0.55)',
            'background:#000',
            'pointer-events:none',
        ].join(';');

        video = document.createElement('video');
        video.id = 'camera';
        video.playsInline = true;
        video.autoplay = true;
        video.muted = true;
        video.style.cssText = [
            'width:100%',
            'height:100%',
            'object-fit:cover',
            'display:block',
            // Зеркалим: повторять движения за незеркальной картинкой
            // невозможно, мозг путает стороны. Точки приходят уже
            // отзеркаленными, поэтому накладываются без поправок.
            'transform:scaleX(-1)',
        ].join(';');

        overlay = document.createElement('canvas');
        overlay.id = 'camera-overlay';
        overlay.style.cssText = 'position:absolute;inset:0;width:100%;height:100%';

        box.appendChild(video);
        box.appendChild(overlay);
        document.body.appendChild(box);
        return video;
    }

    // Отрезки скелета в том же порядке точек, в каком они уходят в игру.
    var BONES = [
        [1, 2], [1, 3], [3, 5], [2, 4], [4, 6],
        [1, 7], [2, 8], [7, 8],
        [7, 9], [9, 11], [8, 10], [10, 12],
    ];

    function drawOverlay() {
        if (!overlay || !latest) return;

        // Размер буфера подгоняем под размер на экране, иначе линии мылит.
        var w = overlay.clientWidth || 1;
        var h = overlay.clientHeight || 1;
        if (overlay.width !== w || overlay.height !== h) {
            overlay.width = w;
            overlay.height = h;
        }

        var g = overlay.getContext('2d');
        g.clearRect(0, 0, w, h);

        // Видео обрезано по короткой стороне (object-fit: cover) — ту же
        // укладку повторяем здесь, иначе скелет разъедется с картинкой.
        var frame = videoAspect;
        var boxAspect = w / h;
        var sx = w, sy = h, ox = 0, oy = 0;
        if (frame > boxAspect) {
            sx = h * frame;
            ox = (w - sx) / 2;
        } else {
            sy = w / frame;
            oy = (h - sy) / 2;
        }

        var at = function (i) {
            return [latest[i * FLOATS_PER_JOINT] * sx + ox,
                    latest[i * FLOATS_PER_JOINT + 1] * sy + oy];
        };
        var seen = function (i) { return latest[i * FLOATS_PER_JOINT + 3] >= 0.5; };

        g.strokeStyle = '#8aa0ff';
        g.lineWidth = 2;
        g.lineCap = 'round';
        for (var b = 0; b < BONES.length; b++) {
            var from = BONES[b][0], to = BONES[b][1];
            if (!seen(from) || !seen(to)) continue;
            var a = at(from), c = at(to);
            g.beginPath();
            g.moveTo(a[0], a[1]);
            g.lineTo(c[0], c[1]);
            g.stroke();
        }

        // Шея: от середины плеч к голове.
        if (seen(0) && seen(1) && seen(2)) {
            var l = at(1), r = at(2), head = at(0);
            g.beginPath();
            g.moveTo((l[0] + r[0]) / 2, (l[1] + r[1]) / 2);
            g.lineTo(head[0], head[1]);
            g.stroke();
        }

        g.fillStyle = '#ffd479';
        for (var i = 0; i < WANTED.length; i++) {
            if (!seen(i)) continue;
            var p = at(i);
            g.beginPath();
            g.arc(p[0], p[1], 2.5, 0, Math.PI * 2);
            g.fill();
        }
    }

    function start() {
        if (status === STATUS.RUNNING || status === STATUS.CAMERA || status === STATUS.MODEL) {
            return;
        }
        lastError = '';
        failedFrames = 0;
        if (!(navigator.mediaDevices && navigator.mediaDevices.getUserMedia)) {
            status = STATUS.UNSUPPORTED;
            return;
        }

        status = STATUS.CAMERA;
        ensureVideo();
        box.style.display = 'block';

        navigator.mediaDevices
            .getUserMedia({
                // Просим немного: распознаванию хватает, а телефон меньше греется.
                video: { facingMode: 'user', width: { ideal: 640 }, height: { ideal: 480 } },
                audio: false,
            })
            .then(function (stream) {
                video.srcObject = stream;
                return video.play();
            })
            .then(loadModel)
            .catch(function (e) {
                var denied = e && (e.name === 'NotAllowedError' || e.name === 'SecurityError');
                failCamera(denied ? STATUS.DENIED : STATUS.FAILED, 'getUserMedia', e);
            });
    }

    // Загрузка идёт четырьмя шагами, и упасть может любой: импорт модуля,
    // wasm-рантайм (9 МБ), файл модели (6 МБ), создание распознавателя.
    // Каждый шаг подписан — иначе по одному «не завелось» причину не найти.
    // Шаг принимает функцию, а не готовый promise: половина отказов здесь
    // синхронные (нет метода, не тот тип), и до .catch они бы не дошли.
    function step(stage, body) {
        try {
            return Promise.resolve(body()).catch(function (e) {
                throw { stage: stage, inner: e };
            });
        } catch (e) {
            return Promise.reject({ stage: stage, inner: e });
        }
    }

    function loadModel() {
        status = STATUS.MODEL;
        var vision = null;

        return step('import', function () {
            return import(MODEL_BASE + '/vision_bundle.mjs');
        })
            .then(function (module) {
                vision = module;
                // Именно forVisionTasks: у FilesetResolver есть ещё
                // forTextTasks и forGenAiExperimentalTasks, но не то, что
                // просится по смыслу.
                return step('wasm', function () {
                    return vision.FilesetResolver.forVisionTasks(MODEL_BASE + '/wasm');
                });
            })
            .then(function (files) {
                // GPU быстрее, но требует своего WebGL-контекста, а один у
                // страницы уже занят игрой. Где так нельзя — уходим на CPU:
                // на lite-модели этого хватает.
                return step('gpu', function () {
                    return create(vision, files, 'GPU');
                }).catch(function (e) {
                    report(e);
                    return step('cpu', function () {
                        return create(vision, files, 'CPU');
                    });
                });
            })
            .then(function (created) {
                landmarker = created;
                latest = new Float32Array(WANTED.length * FLOATS_PER_JOINT);
                status = STATUS.RUNNING;
                requestAnimationFrame(detect);
            })
            .catch(function (e) {
                failModel(e && e.stage ? e.stage : 'model', e && e.inner ? e.inner : e);
            });
    }

    function create(vision, files, delegate) {
        return vision.PoseLandmarker.createFromOptions(files, {
            baseOptions: { modelAssetPath: MODEL_URL, delegate: delegate },
            runningMode: 'VIDEO',
            numPoses: 1,
        });
    }

    /// Неудача, после которой ещё есть запасной путь: причину запоминаем,
    /// но статус не трогаем.
    function report(e) {
        lastError = (e && e.stage ? e.stage : '?') + ' / ' + describe(e && e.inner ? e.inner : e);
    }

    function detect() {
        if (status !== STATUS.RUNNING) return;
        requestAnimationFrame(detect);

        if (!video || video.readyState < 2 || video.currentTime === lastVideoTime) {
            return;
        }
        lastVideoTime = video.currentTime;
        videoAspect = (video.videoWidth || 1) / (video.videoHeight || 1);

        var result;
        try {
            result = landmarker.detectForVideo(video, performance.now());
            failedFrames = 0;
        } catch (e) {
            // Один сбойный кадр — не беда, но если распознавание падает на
            // каждом, режим просто молча ничего не показывает. Такое надо
            // назвать вслух.
            if (++failedFrames >= 30) {
                failModel('detect', e);
            }
            return;
        }
        if (!result || !result.landmarks || !result.landmarks.length) {
            return;
        }

        var points = result.landmarks[0];
        for (var i = 0; i < WANTED.length; i++) {
            var p = points[WANTED[i]];
            var at = i * FLOATS_PER_JOINT;
            // Зеркалим по горизонтали: аватар должен вести себя как отражение,
            // иначе повторять за ним невозможно - мозг путает стороны.
            // Глубину при этом не трогаем: в зеркале переворачивается одна ось,
            // а "ближе к камере" остаётся "ближе к камере".
            latest[at + 0] = p ? 1.0 - p.x : 0.0;
            latest[at + 1] = p ? p.y : 0.0;
            latest[at + 2] = p ? (p.z || 0.0) : 0.0;
            latest[at + 3] = p ? (p.visibility !== undefined ? p.visibility : 1.0) : 0.0;
        }
        latestFresh = true;
        drawOverlay();
    }

    function register_plugin(importObject) {
        importObject.env.pose_start = start;

        importObject.env.pose_stop = function () {
            status = STATUS.IDLE;
            stopCamera();
        };

        importObject.env.pose_status = function () {
            return status;
        };

        // Соотношение сторон кадра нужно игре, чтобы разложить нормированные
        // координаты так же, как браузер разложил видео (object-fit: cover).
        importObject.env.pose_aspect = function () {
            return videoAspect;
        };

        // Текст последней ошибки — в ASCII, другого шрифт движка не знает.
        importObject.env.pose_error = function (ptr, capacity) {
            if (!lastError) return 0;
            var out = new Uint8Array(wasm_memory.buffer, ptr, capacity);
            var count = 0;
            for (var i = 0; i < lastError.length && count < capacity; i++) {
                var code = lastError.charCodeAt(i);
                out[count++] = code >= 32 && code < 127 ? code : 63; // '?'
            }
            return count;
        };

        // Пишем точки прямо в память wasm: так за кадр не создаётся мусора.
        importObject.env.pose_read = function (ptr) {
            if (!latestFresh || !latest) return 0;
            var out = new Float32Array(wasm_memory.buffer, ptr, latest.length);
            out.set(latest);
            latestFresh = false;
            return latest.length / FLOATS_PER_JOINT;
        };
    }

    miniquad_add_plugin({ register_plugin: register_plugin, version: 1, name: 'pose' });
})();
