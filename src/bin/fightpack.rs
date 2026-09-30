//! Packs captured clips for the fighter.
//!   cargo run --release --bin fightpack -- [--character <fighter.glb>] <clips dir> <list.txt> <out.pack>
//! Clips are retargeted onto the fighter's skeleton (assets/character.glb by
//! default), so every fighter gets its own pack with its own strike timing.
//! `list.txt`: one `key = file.glb` per line (`#` comments), optionally
//! followed by `@ from-to` to keep only that window (seconds). Every clip is
//! retargeted onto assets/character.glb and resampled at 30 fps. The report
//! lists, per clip, when each hand and foot reaches furthest forward — the
//! contact candidates used to time clips to the combat frame data.
use some_game::engine::{gltf, math3::*};
use some_game::game::{dancer::Character, mocap::{self, PackClip}};

fn main() {
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    let mut model = "assets/character.glb".to_string();
    if let Some(i) = args.iter().position(|a| a == "--character") {
        if i + 1 < args.len() {
            model = args.remove(i + 1);
        }
        args.remove(i);
    }
    if args.len() < 3 {
        eprintln!("usage: fightpack [--character <fighter.glb>] <clips dir> <list.txt> <out.pack>");
        std::process::exit(2);
    }
    let (dir, list, out) = (&args[0], &args[1], &args[2]);
    let bytes = std::fs::read(&model).unwrap_or_else(|e| panic!("{model}: {e}"));
    let character = Character::from_model(gltf::load_glb(&bytes).expect("character"));
    let skeleton = &character.skeleton;
    let find = |name: &str| skeleton.bones.iter().position(|b| b.name.ends_with(name));
    let limbs = [
        ("hand L", find("LeftHand")),
        ("hand R", find("RightHand")),
        ("foot L", find("LeftToeBase")),
        ("foot R", find("RightToeBase")),
    ];
    let hips = find("Hips").unwrap_or(0);
    let mut clips = Vec::new();
    let text = std::fs::read_to_string(list).expect("list");
    for line in text.lines() {
        let line = line.split('#').next().unwrap_or("").trim();
        let Some((key, rest)) = line.split_once('=') else { continue };
        let (file, window) = rest.split_once('@').unwrap_or((rest, ""));
        let (key, file) = (key.trim(), file.trim());
        let window = window
            .split_once('-')
            .and_then(|(a, b)| Some((a.trim().parse::<f32>().ok()?, b.trim().parse::<f32>().ok()?)));
        let path = std::path::Path::new(dir).join(file);
        let data = match std::fs::read(&path) {
            Ok(d) => d,
            Err(e) => {
                eprintln!("SKIP {key}: {} ({e})", path.display());
                continue;
            }
        };
        let loaded = match gltf::load_clips(&data, skeleton) {
            Ok(c) => c,
            Err(e) => {
                eprintln!("SKIP {key}: {e}");
                continue;
            }
        };
        let Some(mut clip) = loaded
            .into_iter()
            .filter(|c| !c.is_static())
            .max_by(|a, b| a.duration.total_cmp(&b.duration))
        else {
            eprintln!("SKIP {key}: no animation");
            continue;
        };
        // Sampled as a one-shot: a looping clip blends its last key into
        // time zero, so the first packed frame would be the final pose.
        clip.looping = false;
        let (from, to) = window.map_or((0.0, clip.duration), |(a, b)| (a.max(0.0), b.min(clip.duration)));
        // Report: forward reach of each limb over time in character space.
        let rest = skeleton.rest_pose();
        let mut pose = skeleton.rest_pose();
        let mut globals = Vec::new();
        let frames = ((to - from) * 30.0).round() as usize + 1;
        let mut best = [(0usize, f32::MIN); 4];
        let mut first_hips = Vec3::ZERO;
        let mut last_hips = Vec3::ZERO;
        let mut low = f32::MAX;
        for f in 0..frames {
            clip.sample(from + f as f32 / 30.0, &rest, &mut pose);
            skeleton.global_matrices(&pose, &mut globals);
            let at = |b: usize| character.transform.transform_point(globals[b].transform_point(Vec3::ZERO));
            let h = at(hips);
            if f == 0 {
                first_hips = h;
            }
            last_hips = h;
            low = low.min(h.y);
            for (i, (_, bone)) in limbs.iter().enumerate() {
                if let Some(b) = bone {
                    let reach = at(*b).z - h.z;
                    if reach > best[i].1 {
                        best[i] = (f, reach);
                    }
                }
            }
        }
        let travel = last_hips - first_hips;
        let reach: Vec<String> = limbs
            .iter()
            .zip(best)
            .map(|((n, _), (f, r))| format!("{n} f{f} {r:.2}"))
            .collect();
        println!(
            "{key:<14} {:>5.2}s {frames:>3}f | {} | hips travel ({:.2},{:.2},{:.2}) low {:.2}",
            to - from,
            reach.join(" | "),
            travel.x,
            travel.y,
            travel.z,
            low
        );
        clips.push(PackClip { key: key.to_string(), clip, from, to });
    }
    let pack = mocap::write_pack(skeleton, &clips, 30.0);
    std::fs::write(out, &pack).expect("write pack");
    println!("packed {} clips, {} KB -> {out}", clips.len(), pack.len() / 1024);
}
