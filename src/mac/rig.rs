//! The mascot's face rig on macOS: the Windows one (win/notch_ui.rs Mascot, itself a port of ui/mascot.js) without the
//! pointer, the wink or the music notes. Springs pull nine face values (offset, roll, squash, eye opening, smile,
//! hum, eye asymmetry) toward what the face is doing: resting with a slow glance and blinks, or listening to music in a
//! 14 s loop (anticipation, two accented nods, a relaxed sway, humming with the eyes shut). Pure: notch_ui draws it.

pub const X: usize = 0;
pub const Y: usize = 1;
pub const ROLL: usize = 2;
pub const SX: usize = 3;
pub const SY: usize = 4;
pub const OPEN: usize = 5;
pub const SMILE: usize = 6;
pub const HUM: usize = 7;
pub const ASYM: usize = 8;
/// The face at rest: eyes open, a gentle smile.
pub fn rest() -> [f64; 9] { [0.0, 0.0, 0.0, 1.0, 1.0, 1.0, 9.0, 0.0, 0.0] }
pub fn pulse(t: f64, center: f64, width: f64) -> f64 { (-((t - center) / width).powi(2)).exp() }
pub fn smooth(v: f64) -> f64 { let v = v.clamp(0.0, 1.0); v * v * (3.0 - 2.0 * v) }

pub struct Rig { pub face: [f64; 9], vel: [f64; 9], clock: f64, blink_at: f64, blink_start: f64 }
impl Rig {
    pub fn new() -> Self { Self { face: rest(), vel: [0.0; 9], clock: 0.0, blink_at: 2.8, blink_start: -10.0 } }
    /// One step of `dt` seconds (`rand` in 0..1 spaces the blinks; `look`: where the pointer is from the face, -1..1
    /// across and down, while it is near); returns the eyelid (1 open, 0 shut by a blink).
    pub fn step(&mut self, dt: f64, music: bool, rand: f64, look: Option<(f64, f64)>) -> f64 {
        let listening = music && look.is_none();
        let dt = dt.min(0.034);
        self.clock += dt;
        let (c, t) = (self.clock, self.clock % 14.0);
        if c > self.blink_at { self.blink_start = c; self.blink_at = c + 3.2 + rand * 3.8; }
        let b = c - self.blink_start;
        let blink = if !(0.0..=0.26).contains(&b) { 0.0 } else if b < 0.08 { (b / 0.08 * std::f64::consts::FRAC_PI_2).sin() } else if b < 0.115 { 1.0 } else { ((b - 0.115) / 0.145 * std::f64::consts::FRAC_PI_2).cos() };
        let mut target = rest();
        target[Y] = (c * 1.1).sin() * 0.7;
        target[SMILE] = 8.0;
        if let Some((px, py)) = look {
            // Attentive: it turns toward the pointer, eyes a little wider, a bigger smile (the Windows rig's numbers).
            target[X] = px * 7.0; target[Y] = py * 3.0 - 1.0; target[ROLL] = px * 3.0; target[OPEN] = 1.05; target[SMILE] = 11.0;
            target[SY] = 1.025; target[ASYM] = px * 0.05;
        } else if music {
            // Anticipation, two accented nods, a relaxed sway, humming.
            let nod = pulse(t, 1.2, 0.22) + 0.8 * pulse(t, 1.85, 0.19) + pulse(t, 8.1, 0.23) + 0.65 * pulse(t, 8.7, 0.18);
            let anticipate = pulse(t, 0.85, 0.22) + pulse(t, 7.75, 0.22);
            let sway = (c * 2.25).sin() * (std::f64::consts::PI * t / 14.0).sin().powi(2);
            let hum = pulse(t, 5.1, 0.8);
            target = [sway * 4.5, nod * 4.3 - anticipate * 2.4 + (c * 4.5).sin() * 0.7, sway * 7.0 + (c * 4.5).sin() * 1.5,
                1.0 + nod * 0.07 - anticipate * 0.025, 1.0 - nod * 0.08 + anticipate * 0.06, 0.0, 10.0 + nod * 3.0 - hum * 7.0, hum, 0.0];
        } else {
            let glance = (c * 0.46).sin();
            target[X] = glance * 2.0;
            target[ROLL] = glance * 0.7;
            target[ASYM] = pulse(t, 9.0, 0.6) * 0.12;
        }
        for i in 0..9 {
            let (k, d) = if i == OPEN { (260.0, 32.0) } else { (170.0, 23.0) };
            self.vel[i] += (k * (target[i] - self.face[i]) - d * self.vel[i]) * dt;
            self.face[i] += self.vel[i] * dt;
        }
        if listening { 1.0 } else { 1.0 - blink }
    }
}

/// The 7 points of one eye (two cubic segments): a vertical stroke when open, closing into a smile curve.
pub fn eye_points(cx: f64, cy: f64, open: f64, tilt: f64) -> [(f64, f64); 7] {
    let amount = open.clamp(0.0, 1.0);
    let curve = 1.0 - smooth((amount / 0.4).min(1.0));
    let rise = smooth(((amount - 0.4) / 0.6).max(0.0));
    let (x, y, lean) = (10.0 * curve, 10.0 * rise, tilt * curve);
    let p = [(cx - x, cy - y + lean), (cx - x * 0.5, cy + 8.0 * curve), (cx + x * 0.5, cy + 8.0 * curve), (cx + x, cy + y - lean)];
    let mid = |a: (f64, f64), b: (f64, f64)| ((a.0 + b.0) / 2.0, (a.1 + b.1) / 2.0);
    let (a, b, c) = (mid(p[0], p[1]), mid(p[1], p[2]), mid(p[2], p[3]));
    let (d, e) = (mid(a, b), mid(b, c));
    [p[0], a, d, mid(d, e), e, c, p[3]]
}

/// The face drawn: each eye's points and stroke width, the mouth's four points (a cubic) and width. In the Windows
/// mascot's 160 x 70 units, y down.
pub struct Pose { pub eyes: [[(f64, f64); 7]; 2], pub eye_width: [f64; 2], pub mouth: [(f64, f64); 4], pub mouth_width: f64 }
/// Where the face's values put its strokes (pure): the lid squeezes the eyes about their middle line; the whole face is
/// squashed, rolled (degrees) and moved about its centre (80, 34).
pub fn pose(f: &[f64; 9], lid: f64) -> Pose {
    let (sin, cos) = f[ROLL].to_radians().sin_cos();
    let rig = |(x, y): (f64, f64)| { let (x, y) = ((x - 80.0) * f[SX], (y - 34.0) * f[SY]); (x * cos - y * sin + 80.0 + f[X], x * sin + y * cos + 34.0 + f[Y]) };
    let k = 0.24 + 0.76 * lid;
    let eye = |cx: f64, open: f64, tilt: f64| eye_points(cx, 31.0, open, tilt).map(|(x, y)| rig((x, 31.0 + (y - 31.0) * k)));
    let (l, r) = (f[OPEN] + f[ASYM], f[OPEN] - f[ASYM]);
    let half = 16.0 - 7.0 * f[HUM];
    let mouth = [(80.0 - half, 39.0), (80.0 - half * 0.6, 39.0 + f[SMILE]), (80.0 + half * 0.6, 39.0 + f[SMILE]), (80.0 + half, 39.0)].map(rig);
    Pose { eyes: [eye(42.0, l, f[ROLL] * 0.07), eye(118.0, r, -f[ROLL] * 0.07)], eye_width: [9.0 + 5.0 * l.clamp(0.0, 1.0), 9.0 + 5.0 * r.clamp(0.0, 1.0)], mouth, mouth_width: 3.8 + f[HUM] * 1.2 }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn eyes_open_upright_and_close_into_a_smile() {
        let open = eye_points(42.0, 31.0, 1.0, 0.0);
        assert!((open[0].0 - 42.0).abs() < 1e-9 && (open[6].0 - 42.0).abs() < 1e-9 && open[0].1 < open[6].1, "open: a vertical stroke");
        let shut = eye_points(42.0, 31.0, 0.0, 0.0);
        assert!(shut[0].0 < 33.0 && shut[6].0 > 51.0 && shut[3].1 > 31.0, "shut: a smile curve");
    }
    #[test]
    fn the_rig_rests_and_listens_like_on_windows() {
        let mut m = Rig::new();
        for _ in 0..300 { m.step(1.0 / 60.0, false, 0.5, None); }
        assert!(m.face[OPEN] > 0.9 && m.face[SMILE] > 7.0, "awake at rest");
        let (mut roll, mut rise) = (0.0f64, 0.0f64);
        for _ in 0..(14 * 60) { assert_eq!(m.step(1.0 / 60.0, true, 0.5, None), 1.0, "no blinking while listening"); roll = roll.max(m.face[ROLL].abs()); rise = rise.max(m.face[Y]); }
        assert!(m.face[OPEN] < 0.2, "eyes shut while listening");
        assert!(roll > 3.0 && rise > 2.0, "it sways ({roll}) and nods ({rise}) over the 14 s loop");
        for _ in 0..120 { m.step(1.0 / 60.0, true, 0.5, Some((1.0, 0.0))); }
        assert!(m.face[X] > 5.0 && m.face[OPEN] > 0.9, "a pointer near: it looks that way, eyes open, music or not");
        let p = pose(&rest(), 1.0);
        assert_eq!(p.eyes[0][0], (42.0, 21.0));
        assert_eq!((p.mouth[0], p.mouth[3]), ((64.0, 39.0), (96.0, 39.0)), "the resting smile");
    }
}
