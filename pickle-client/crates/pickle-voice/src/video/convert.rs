//! Pixel format conversion into I420 (what the H.264 encoder eats) and a
//! cheap downscaled RGBA preview. Integer-only BT.601 limited range.

/// Even-sized I420 buffer.
pub struct I420 {
    pub width: usize,
    pub height: usize,
    pub data: Vec<u8>,
}

impl I420 {
    pub fn new(width: usize, height: usize) -> Self {
        let width = width & !1;
        let height = height & !1;
        Self { width, height, data: vec![0; width * height * 3 / 2] }
    }

    pub fn planes(&self) -> (&[u8], &[u8], &[u8]) {
        let ys = self.width * self.height;
        let cs = ys / 4;
        (&self.data[..ys], &self.data[ys..ys + cs], &self.data[ys + cs..])
    }

    fn planes_mut(&mut self) -> (&mut [u8], &mut [u8], &mut [u8]) {
        let ys = self.width * self.height;
        let cs = ys / 4;
        let (y, rest) = self.data.split_at_mut(ys);
        let (u, v) = rest.split_at_mut(cs);
        (y, u, v)
    }
}

#[inline]
fn rgb_to_y(r: i32, g: i32, b: i32) -> u8 {
    (((66 * r + 129 * g + 25 * b + 128) >> 8) + 16).clamp(0, 255) as u8
}
#[inline]
fn rgb_to_u(r: i32, g: i32, b: i32) -> u8 {
    (((-38 * r - 74 * g + 112 * b + 128) >> 8) + 128).clamp(0, 255) as u8
}
#[inline]
fn rgb_to_v(r: i32, g: i32, b: i32) -> u8 {
    (((112 * r - 94 * g - 18 * b + 128) >> 8) + 128).clamp(0, 255) as u8
}

/// Packed YUYV (YUY2) → I420. Chroma of the two rows is averaged.
pub fn yuyv_to_i420(src: &[u8], src_w: usize, dst: &mut I420) {
    let (w, h) = (dst.width, dst.height);
    let stride = src_w * 2;
    let (yp, up, vp) = dst.planes_mut();
    for row in 0..h {
        let line = &src[row * stride..row * stride + w * 2];
        let yrow = &mut yp[row * w..row * w + w];
        for (x, px) in line.chunks_exact(4).enumerate() {
            yrow[2 * x] = px[0];
            yrow[2 * x + 1] = px[2];
        }
    }
    for row in (0..h).step_by(2) {
        let a = &src[row * stride..];
        let b = &src[(row + 1) * stride..];
        let cw = w / 2;
        for x in 0..cw {
            let i = x * 4;
            up[(row / 2) * cw + x] = (a[i + 1] as u16 + b[i + 1] as u16).div_ceil(2) as u8;
            vp[(row / 2) * cw + x] = (a[i + 3] as u16 + b[i + 3] as u16).div_ceil(2) as u8;
        }
    }
}

/// NV12 → I420 (de-interleave chroma).
pub fn nv12_to_i420(src: &[u8], src_w: usize, src_h: usize, dst: &mut I420) {
    let (w, h) = (dst.width, dst.height);
    let (yp, up, vp) = dst.planes_mut();
    for row in 0..h {
        yp[row * w..row * w + w].copy_from_slice(&src[row * src_w..row * src_w + w]);
    }
    let uv = &src[src_w * src_h..];
    let cw = w / 2;
    for row in 0..h / 2 {
        let line = &uv[row * src_w..];
        for x in 0..cw {
            up[row * cw + x] = line[2 * x];
            vp[row * cw + x] = line[2 * x + 1];
        }
    }
}

/// Packed RGB24 → I420.
pub fn rgb_to_i420(src: &[u8], src_w: usize, dst: &mut I420) {
    let (w, h) = (dst.width, dst.height);
    let (yp, up, vp) = dst.planes_mut();
    let stride = src_w * 3;
    for row in 0..h {
        let line = &src[row * stride..row * stride + w * 3];
        for (x, px) in line.chunks_exact(3).enumerate() {
            yp[row * w + x] = rgb_to_y(px[0] as i32, px[1] as i32, px[2] as i32);
        }
    }
    let cw = w / 2;
    for row in (0..h).step_by(2) {
        for x in 0..cw {
            let mut acc = [0i32; 3];
            for (dy, dx) in [(0, 0), (0, 1), (1, 0), (1, 1)] {
                let i = (row + dy) * stride + (2 * x + dx) * 3;
                acc[0] += src[i] as i32;
                acc[1] += src[i + 1] as i32;
                acc[2] += src[i + 2] as i32;
            }
            let (r, g, b) = (acc[0] / 4, acc[1] / 4, acc[2] / 4);
            up[(row / 2) * cw + x] = rgb_to_u(r, g, b);
            vp[(row / 2) * cw + x] = rgb_to_v(r, g, b);
        }
    }
}

/// Nearest-neighbour I420 → RGBA preview, at most `max_w` wide.
pub fn i420_preview_rgba(src: &I420, max_w: usize) -> (usize, usize, Vec<u8>) {
    let scale = (src.width.div_ceil(max_w.max(1))).max(1);
    let (ow, oh) = (src.width / scale, src.height / scale);
    let (yp, up, vp) = src.planes();
    let cw = src.width / 2;
    let mut out = Vec::with_capacity(ow * oh * 4);
    for oy in 0..oh {
        let sy = oy * scale;
        for ox in 0..ow {
            let sx = ox * scale;
            let y = yp[sy * src.width + sx] as i32 - 16;
            let u = up[(sy / 2) * cw + sx / 2] as i32 - 128;
            let v = vp[(sy / 2) * cw + sx / 2] as i32 - 128;
            let c = 298 * y;
            out.push(((c + 409 * v + 128) >> 8).clamp(0, 255) as u8);
            out.push(((c - 100 * u - 208 * v + 128) >> 8).clamp(0, 255) as u8);
            out.push(((c + 516 * u + 128) >> 8).clamp(0, 255) as u8);
            out.push(255);
        }
    }
    (ow, oh, out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rgb_white_and_black() {
        let mut dst = I420::new(2, 2);
        rgb_to_i420(&[255; 12], 2, &mut dst);
        assert_eq!(dst.planes().0, &[235, 235, 235, 235]);
        assert_eq!(dst.planes().1, &[128]);
        rgb_to_i420(&[0; 12], 2, &mut dst);
        assert_eq!(dst.planes().0, &[16, 16, 16, 16]);
    }

    #[test]
    fn yuyv_and_nv12_layouts() {
        // 2x2 YUYV: Y0 U Y1 V per row
        let yuyv = [10, 100, 20, 200, 30, 110, 40, 210];
        let mut dst = I420::new(2, 2);
        yuyv_to_i420(&yuyv, 2, &mut dst);
        assert_eq!(dst.planes(), (&[10u8, 20, 30, 40][..], &[105u8][..], &[205u8][..]));
        let nv12 = [1, 2, 3, 4, 50, 60];
        nv12_to_i420(&nv12, 2, 2, &mut dst);
        assert_eq!(dst.planes(), (&[1u8, 2, 3, 4][..], &[50u8][..], &[60u8][..]));
    }

    #[test]
    fn preview_downscales() {
        let src = I420::new(1920, 1080);
        let (w, h, px) = i420_preview_rgba(&src, 640);
        assert_eq!((w, h), (640, 360));
        assert_eq!(px.len(), 640 * 360 * 4);
    }
}
