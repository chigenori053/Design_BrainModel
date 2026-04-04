use crate::model::{RuntimePath3D, Vec3};

pub fn animated_path(path: &RuntimePath3D, time: f64) -> Vec<Vec3> {
    if !path.animated || path.points.len() <= 2 {
        return path.points.clone();
    }
    let phase = ((time * 0.45) as f32).fract();
    let visible = ((path.points.len() as f32 - 1.0) * phase).ceil() as usize + 1;
    path.points
        .iter()
        .take(visible.clamp(2, path.points.len()))
        .copied()
        .collect()
}

pub fn morph(from: Vec3, to: Vec3, time: f64) -> Vec3 {
    let t = ((time * 0.35) as f32).sin() * 0.5 + 0.5;
    Vec3 {
        x: from.x + (to.x - from.x) * t,
        y: from.y + (to.y - from.y) * t,
        z: from.z + (to.z - from.z) * t,
    }
}

pub fn reverse_delta_replay(points: &[Vec3]) -> Vec<Vec3> {
    points.iter().rev().copied().collect()
}

pub fn reverse_replay_from_checkpoint(checkpoint: &[Vec3], delta_path: &[Vec3]) -> Vec<Vec3> {
    checkpoint
        .iter()
        .copied()
        .chain(reverse_delta_replay(delta_path))
        .collect()
}

#[cfg(test)]
mod tests {
    use crate::model::Vec3;

    #[test]
    fn reverse_delta_replay() {
        let path = vec![
            Vec3 {
                x: 0.0,
                y: 0.0,
                z: 0.0,
            },
            Vec3 {
                x: 1.0,
                y: 2.0,
                z: 3.0,
            },
            Vec3 {
                x: 4.0,
                y: 5.0,
                z: 6.0,
            },
        ];
        let reversed = super::reverse_delta_replay(&path);
        assert_eq!(reversed[0].x, 4.0);
        assert_eq!(reversed[2].x, 0.0);
    }

    #[test]
    fn reverse_replay_from_checkpoint() {
        let checkpoint = vec![Vec3 {
            x: 9.0,
            y: 0.0,
            z: 0.0,
        }];
        let path = vec![
            Vec3 {
                x: 1.0,
                y: 0.0,
                z: 0.0,
            },
            Vec3 {
                x: 2.0,
                y: 0.0,
                z: 0.0,
            },
        ];
        let replay = super::reverse_replay_from_checkpoint(&checkpoint, &path);
        assert_eq!(replay.len(), 3);
        assert_eq!(replay[0].x, 9.0);
        assert_eq!(replay[1].x, 2.0);
        assert_eq!(replay[2].x, 1.0);
    }
}
