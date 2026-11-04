//! The waveform kfbox draws: `[low, mid, high, level]` columns.
//!
//! The analyser's columns are peaks, which suit a CDJ's file but not a
//! picture: a mastered track peaks near full scale from the first kick to the
//! last, so every column drew full height, and its bands peak together, so
//! every column drew the same pale colour. These columns measure energy
//! instead. The level is the column's RMS over the track's own loud end (its
//! 95th percentile, so one hit doesn't set the scale), on a gentle curve, so
//! builds, breaks and drops read. Each band is its RMS over that band's own
//! loud end, so a column's colour says which bands are strong *for this
//! track*: a kick and bass section leans red, a hat-led one blue.

/// The percentile that counts as a track's loud end.
const LOUD_END: f64 = 0.95;
/// The level's curve: under 1 lifts quiet columns a little, so a break
/// stays visible without reaching a drop's height.
const LEVEL_CURVE: f64 = 0.8;
/// The least a band's loud end is taken to be, against the whole signal's:
/// a band the track hardly has isn't scaled up until its leakage shows.
const BAND_FLOOR: f64 = 0.3;
/// Band crossovers, in Hz, as the analyser's.
const LOW_MID: f64 = 200.0;
const MID_HIGH: f64 = 2000.0;

/// A second-order Butterworth section.
struct Biquad {
    b: [f64; 3],
    a: [f64; 2],
    z: [f64; 2],
}

impl Biquad {
    fn new(rate: u32, cutoff: f64, highpass: bool) -> Self {
        let rate = f64::from(rate);
        let w = 2.0 * std::f64::consts::PI * cutoff.min(rate * 0.45) / rate;
        let (sin, cos) = w.sin_cos();
        let alpha = sin / std::f64::consts::SQRT_2;
        let a0 = 1.0 + alpha;
        let b = if highpass {
            [f64::midpoint(1.0, cos), -(1.0 + cos), f64::midpoint(1.0, cos)]
        } else {
            [(1.0 - cos) / 2.0, 1.0 - cos, (1.0 - cos) / 2.0]
        };
        Self { b: b.map(|v| v / a0), a: [-2.0 * cos / a0, (1.0 - alpha) / a0], z: [0.0; 2] }
    }

    fn process(&mut self, x: f64) -> f64 {
        let y = self.b[0] * x + self.z[0];
        self.z[0] = self.b[1] * x - self.a[0] * y + self.z[1];
        self.z[1] = self.b[2] * x - self.a[1] * y;
        y
    }
}

/// The value `fraction` of the way up, of the columns that hold audio.
fn percentile(values: impl Iterator<Item = f64>, fraction: f64) -> f64 {
    let mut sorted: Vec<f64> = values.collect();
    if sorted.is_empty() {
        return 0.0;
    }
    sorted.sort_by(f64::total_cmp);
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss, clippy::cast_precision_loss, reason = "an index into the list")]
    let at = ((sorted.len() - 1) as f64 * fraction).round() as usize;
    sorted[at]
}

/// `count` columns of `[low, mid, high, level]`, 0 to 255, across `samples`.
#[must_use]
pub fn columns(samples: &[f32], rate: u32, count: usize) -> Vec<[u8; 4]> {
    if count == 0 || samples.is_empty() || rate == 0 {
        return vec![[0; 4]; count];
    }
    let mut low = Biquad::new(rate, LOW_MID, false);
    let (mut mid_low, mut mid_high) = (Biquad::new(rate, LOW_MID, true), Biquad::new(rate, MID_HIGH, false));
    let mut high = Biquad::new(rate, MID_HIGH, true);
    let mut sums = vec![[0.0_f64; 4]; count];
    let mut counts = vec![0_usize; count];
    for (i, &sample) in samples.iter().enumerate() {
        let x = f64::from(sample);
        let bands = [low.process(x), mid_high.process(mid_low.process(x)), high.process(x), x];
        let column = (i * count / samples.len()).min(count - 1);
        for (sum, band) in sums[column].iter_mut().zip(bands) {
            *sum += band * band;
        }
        counts[column] += 1;
    }
    #[allow(clippy::cast_precision_loss, reason = "a sample count")]
    let rms: Vec<[f64; 4]> = sums.iter().zip(&counts).map(|(sum, &n)| sum.map(|s| (s / n.max(1) as f64).sqrt())).collect();
    let heard = || rms.iter().zip(&counts).filter(|(_, &n)| n > 0).map(|(r, _)| r);
    let level = percentile(heard().map(|r| r[3]), LOUD_END);
    let loud: [f64; 4] =
        std::array::from_fn(|band| if band == 3 { level } else { percentile(heard().map(|r| r[band]), LOUD_END).max(level * BAND_FLOOR) });
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss, reason = "clamped to 0..=255")]
    let byte = |v: f64| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
    rms.iter()
        .map(|r| {
            let scaled: [f64; 4] = std::array::from_fn(|band| if loud[band] > 1e-9 { r[band] / loud[band] } else { 0.0 });
            [byte(scaled[0]), byte(scaled[1]), byte(scaled[2]), byte(scaled[3].min(1.0).powf(LEVEL_CURVE))]
        })
        .collect()
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::cast_precision_loss, reason = "tests")]
mod tests {
    use super::*;

    const RATE: u32 = 44_100;

    fn tone(hz: f64, amplitude: f64, secs: f64) -> Vec<f32> {
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let n = (secs * f64::from(RATE)) as usize;
        #[allow(clippy::cast_possible_truncation)]
        (0..n).map(|i| (amplitude * (2.0 * std::f64::consts::PI * hz * i as f64 / f64::from(RATE)).sin()) as f32).collect()
    }

    #[test]
    fn a_quieter_section_draws_lower_even_when_both_peak_near_full_scale() {
        // A "drop": a full-scale square-ish wave; a "break": the same peak,
        // but only for a moment in each column, so far less energy.
        let mut samples = tone(60.0, 0.98, 4.0);
        let mut quiet = tone(60.0, 0.98, 4.0);
        for (i, s) in quiet.iter_mut().enumerate() {
            if i % 4410 > 400 {
                *s *= 0.15;
            }
        }
        samples.extend(quiet);
        let columns = columns(&samples, RATE, 80);
        let drop = columns[..40].iter().map(|c| c[3]).min().unwrap();
        let brk = columns[45..].iter().map(|c| c[3]).max().unwrap();
        assert!(drop > 240, "the loud half reaches the top: {drop}");
        assert!(brk < 140, "the quiet half has room to read as quieter: {brk}");
        assert!(brk > 40, "and is still visible: {brk}");
    }

    #[test]
    fn bass_reads_as_low_band_and_hats_as_high_band() {
        let mut samples = tone(60.0, 0.6, 2.0);
        samples.extend(tone(8000.0, 0.6, 2.0));
        // Both bands present at their own loud ends, so each is normalised.
        samples.extend(tone(60.0, 0.6, 2.0).iter().zip(tone(8000.0, 0.6, 2.0)).map(|(a, b)| a + b));
        let columns = columns(&samples, RATE, 60);
        let [low, mid, high, _] = columns[10];
        assert!(low > 200 && high < 30 && mid < 60, "bass section: {:?}", columns[10]);
        let [low, _, high, _] = columns[30];
        assert!(high > 200 && low < 30, "hat section: {:?}", columns[30]);
        let [low, _, high, _] = columns[50];
        assert!(low > 200 && high > 200, "both: {:?}", columns[50]);
    }

    #[test]
    fn silence_and_nothing_draw_nothing() {
        assert_eq!(columns(&[], RATE, 3), vec![[0; 4]; 3]);
        assert_eq!(columns(&[0.0; 1000], RATE, 4), vec![[0; 4]; 4]);
        assert_eq!(columns(&[0.5; 10], RATE, 0), Vec::<[u8; 4]>::new());
    }
}
