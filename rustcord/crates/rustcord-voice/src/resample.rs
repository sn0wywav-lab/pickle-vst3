//! Streaming polyphase windowed-sinc resampler (interleaved f32).
//!
//! Only used when a device cannot run at 48 kHz. When capture and Opus both
//! run at 48 kHz the samples bypass this entirely (bit-transparent path).

const HALF_TAPS: usize = 24;
const TAPS: usize = HALF_TAPS * 2;
const PHASES: usize = 256;

pub struct Resampler {
    channels: usize,
    step: f64,
    pos: f64,
    hist: Vec<f32>,
    table: Vec<f32>,
}

fn sinc(x: f64) -> f64 {
    if x.abs() < 1e-9 {
        1.0
    } else {
        let px = std::f64::consts::PI * x;
        px.sin() / px
    }
}

/// 4-term Blackman-Harris window on [-1, 1].
fn window(x: f64) -> f64 {
    if x.abs() >= 1.0 {
        return 0.0;
    }
    let t = (x + 1.0) * 0.5; // 0..1
    let tau = 2.0 * std::f64::consts::PI * t;
    0.35875 - 0.48829 * tau.cos() + 0.14128 * (2.0 * tau).cos() - 0.01168 * (3.0 * tau).cos()
}

impl Resampler {
    pub fn new(in_rate: u32, out_rate: u32, channels: usize) -> Self {
        let step = in_rate as f64 / out_rate as f64;
        // Anti-alias when downsampling; keep 0.95 of Nyquist otherwise.
        let cutoff = (out_rate as f64 / in_rate as f64).min(1.0) * 0.95;
        let mut table = vec![0f32; (PHASES + 1) * TAPS];
        for p in 0..=PHASES {
            let frac = p as f64 / PHASES as f64;
            let mut row = [0f64; TAPS];
            let mut sum = 0.0;
            for (k, r) in row.iter_mut().enumerate() {
                let d = frac + (HALF_TAPS as f64 - 1.0) - k as f64;
                *r = cutoff * sinc(cutoff * d) * window(d / HALF_TAPS as f64);
                sum += *r;
            }
            for (k, r) in row.iter().enumerate() {
                table[p * TAPS + k] = (*r / sum) as f32;
            }
        }
        Self {
            channels,
            step,
            pos: (HALF_TAPS - 1) as f64,
            hist: vec![0.0; (HALF_TAPS - 1) * channels],
            table,
        }
    }

    pub fn is_passthrough(&self) -> bool {
        (self.step - 1.0).abs() < 1e-12
    }

    /// Resample `input` (interleaved) appending to `output`.
    pub fn process(&mut self, input: &[f32], output: &mut Vec<f32>) {
        if self.is_passthrough() {
            output.extend_from_slice(input);
            return;
        }
        let ch = self.channels;
        self.hist.extend_from_slice(input);
        let frames = self.hist.len() / ch;
        output.reserve(((input.len() / ch) as f64 / self.step) as usize * ch + ch * 2);
        while (self.pos as usize) + HALF_TAPS < frames {
            let ip = self.pos as usize;
            let frac = self.pos - ip as f64;
            let ph = frac * PHASES as f64;
            let p0 = ph as usize;
            let w = (ph - p0 as f64) as f32;
            let row0 = &self.table[p0 * TAPS..p0 * TAPS + TAPS];
            let row1 = &self.table[(p0 + 1) * TAPS..(p0 + 1) * TAPS + TAPS];
            let base = ip + 1 - HALF_TAPS;
            for c in 0..ch {
                let mut acc = 0f32;
                for k in 0..TAPS {
                    let coef = row0[k] + (row1[k] - row0[k]) * w;
                    acc += self.hist[(base + k) * ch + c] * coef;
                }
                output.push(acc);
            }
            self.pos += self.step;
        }
        // Drop consumed history.
        let keep_from = (self.pos as usize).saturating_sub(HALF_TAPS - 1);
        if keep_from > 0 {
            self.hist.drain(..keep_from * ch);
            self.pos -= keep_from as f64;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rms(x: &[f32]) -> f32 {
        (x.iter().map(|v| v * v).sum::<f32>() / x.len() as f32).sqrt()
    }

    #[test]
    fn upsample_44k1_to_48k_preserves_tone() {
        let mut r = Resampler::new(44_100, 48_000, 1);
        let tone: Vec<f32> = (0..44_100)
            .map(|i| (2.0 * std::f32::consts::PI * 1000.0 * i as f32 / 44_100.0).sin() * 0.5)
            .collect();
        let mut out = Vec::new();
        for chunk in tone.chunks(441) {
            r.process(chunk, &mut out);
        }
        // Output length ~= 48000 (minus filter delay)
        assert!((47_900..=48_000).contains(&out.len()), "{}", out.len());
        let body = &out[2000..40_000];
        assert!((rms(body) - 0.3535).abs() < 0.01, "{}", rms(body));
        // Count zero crossings => frequency stays 1 kHz
        let zc = body.windows(2).filter(|w| w[0] <= 0.0 && w[1] > 0.0).count();
        let secs = body.len() as f32 / 48_000.0;
        assert!(((zc as f32 / secs) - 1000.0).abs() < 5.0);
    }

    #[test]
    fn passthrough_is_bit_exact() {
        let mut r = Resampler::new(48_000, 48_000, 2);
        let data: Vec<f32> = (0..100).map(|i| i as f32 * 0.001).collect();
        let mut out = Vec::new();
        r.process(&data, &mut out);
        assert_eq!(out, data);
    }
}
