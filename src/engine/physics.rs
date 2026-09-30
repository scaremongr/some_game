//! Fixed-step presentation physics: constrained bodies and bouncing debris.
//! Competitive movement and wall damage are authoritative in arena-combat.
use super::math3::*;
use super::pose::{Joint, Pose, JOINT_COUNT};

pub struct Ragdoll {
    pub points: [Vec3; JOINT_COUNT],
    previous: [Vec3; JOINT_COUNT],
    links: Vec<(usize, usize, f32)>,
    age: f32,
}
impl Ragdoll {
    pub fn new(pose: &Pose, velocity: Vec3) -> Self {
        let points = std::array::from_fn(|i| pose.get(Joint::ALL[i]));
        let previous = std::array::from_fn(|i| points[i] - velocity * (1.0 / 60.0));
        use Joint::*;
        let pairs = [
            (Head, ShoulderL),
            (Head, ShoulderR),
            (ShoulderL, ShoulderR),
            (ShoulderL, ElbowL),
            (ElbowL, WristL),
            (ShoulderR, ElbowR),
            (ElbowR, WristR),
            (ShoulderL, HipL),
            (ShoulderR, HipR),
            (ShoulderL, HipR),
            (ShoulderR, HipL),
            (HipL, HipR),
            (HipL, KneeL),
            (KneeL, AnkleL),
            (HipR, KneeR),
            (KneeR, AnkleR),
        ];
        let links = pairs
            .iter()
            .map(|(a, b)| {
                (
                    a.index(),
                    b.index(),
                    (points[a.index()] - points[b.index()]).length(),
                )
            })
            .collect();
        Self {
            points,
            previous,
            links,
            age: 0.0,
        }
    }
    pub fn step(&mut self) {
        if self.age > 6.0 {
            return;
        }
        self.age += 1.0 / 60.0;
        for i in 0..JOINT_COUNT {
            let velocity = (self.points[i] - self.previous[i]) * 0.988;
            self.previous[i] = self.points[i];
            self.points[i] += velocity + vec3(0.0, -9.81 / (60.0 * 60.0), 0.0);
        }
        for _ in 0..10 {
            for &(a, b, length) in &self.links {
                let delta = self.points[b] - self.points[a];
                let distance = delta.length().max(0.0001);
                let correction = delta * ((distance - length) / distance * 0.5);
                self.points[a] += correction;
                self.points[b] += -correction;
            }
            for i in 0..JOINT_COUNT {
                let radius = if i == Joint::Head.index() {
                    0.13
                } else {
                    0.065
                };
                if self.points[i].y < radius {
                    self.points[i].y = radius;
                    self.previous[i].x += (self.points[i].x - self.previous[i].x) * 0.35;
                    self.previous[i].z += (self.points[i].z - self.previous[i].z) * 0.35;
                }
            }
        }
    }
    pub fn pose(&self) -> Pose {
        let mut result = Pose::default();
        for joint in Joint::ALL {
            result.set(joint, self.points[joint.index()], 1.0);
        }
        result
    }
}

#[derive(Clone)]
pub struct Fragment {
    pub position: Vec3,
    pub velocity: Vec3,
    pub rotation: Quat,
    pub spin: Vec3,
    pub size: Vec3,
    pub age: u32,
}
impl Fragment {
    pub fn step(&mut self) {
        if self.age >= 360 {
            return;
        }
        self.age += 1;
        self.velocity.y -= 9.81 / 60.0;
        self.position += self.velocity * (1.0 / 60.0);
        let speed = self.spin.length();
        if speed > 0.01 {
            self.rotation = (Quat::from_axis_angle(self.spin.normalize(), speed / 60.0)
                * self.rotation)
                .normalize();
        }
        // Support of the rotated box against the floor, so a settled fragment
        // rests on a real face/edge instead of hovering on a bounding sphere.
        let radius = self.support_height();
        if self.position.y < radius {
            self.position.y = radius;
            self.velocity.y = self.velocity.y.abs() * 0.27;
            self.velocity.x *= 0.78;
            self.velocity.z *= 0.78;
            self.spin = self.spin * 0.76;
            if self.velocity.length() < 0.08 {
                self.velocity = Vec3::ZERO;
                self.spin = Vec3::ZERO;
                self.age = 360;
            }
        }
    }
    pub fn matrix(&self) -> Mat4 {
        Mat4::from_trs(self.position, self.rotation, self.size)
    }
    pub fn support_height(&self) -> f32 {
        (self.rotation.rotate(Vec3::X).y.abs() * self.size.x
            + self.rotation.rotate(Vec3::Y).y.abs() * self.size.y
            + self.rotation.rotate(Vec3::Z).y.abs() * self.size.z)
            * 0.5
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn fragments_bounce_and_settle_without_falling_through_floor() {
        let mut f = Fragment {
            position: vec3(0.0, 2.0, 0.0),
            velocity: vec3(3.0, 1.0, 0.0),
            rotation: Quat::IDENTITY,
            spin: vec3(1.0, 2.0, 3.0),
            size: vec3(0.2, 0.3, 0.2),
            age: 0,
        };
        for _ in 0..400 {
            f.step();
            assert!(f.position.y >= f.support_height() - 0.001);
        }
        assert!(f.position.x > 0.5);
        assert_eq!(f.age, 360);
    }
    #[test]
    fn ragdoll_preserves_bone_lengths_after_impact() {
        let mut pose = Pose::default();
        for (i, j) in Joint::ALL.iter().enumerate() {
            pose.set(
                *j,
                vec3((i % 2) as f32 * 0.2, 1.8 - i as f32 * 0.1, 0.0),
                1.0,
            );
        }
        let mut body = Ragdoll::new(&pose, vec3(3.0, 2.0, 0.3));
        for _ in 0..360 {
            body.step();
        }
        for &(a, b, len) in &body.links {
            assert!(((body.points[a] - body.points[b]).length() - len).abs() < 0.06);
        }
        assert!(body.points.iter().all(|p| p.y >= 0.06 && p.x.is_finite()));
    }
}
