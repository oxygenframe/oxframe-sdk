//! Gaussian blur approximated by three box blurs (error < 3 %), on
//! premultiplied RGBA8. Linear in the image size, independent of radius.

/// Box sizes for 3 passes approximating a Gaussian of `sigma` (W. Jarosz).
fn boxes(sigma: f64) -> [usize; 3] {
    let n = 3.0;
    let ideal = (12.0 * sigma * sigma / n + 1.0).sqrt();
    let mut wl = ideal.floor() as i64;
    if wl % 2 == 0 {
        wl -= 1;
    }
    let wu = wl + 2;
    let m = ((12.0 * sigma * sigma - n * (wl * wl) as f64 - 4.0 * n * wl as f64 - 3.0 * n)
        / (-4.0 * wl as f64 - 4.0))
        .round() as i64;
    let mut out = [0usize; 3];
    for (i, o) in out.iter_mut().enumerate() {
        *o = (if (i as i64) < m { wl } else { wu }).max(1) as usize;
    }
    out
}

/// One horizontal box pass of radius `r` from `src` into `dst` (RGBA rows).
fn box_h(src: &[u8], dst: &mut [u8], w: usize, h: usize, r: usize) {
    let div = (2 * r + 1) as u32;
    for y in 0..h {
        let row = y * w * 4;
        for c in 0..4 {
            let at = |x: isize| -> u32 {
                let x = x.clamp(0, w as isize - 1) as usize;
                u32::from(src[row + x * 4 + c])
            };
            let mut acc: u32 = (-(r as isize)..=r as isize).map(at).sum();
            for x in 0..w {
                dst[row + x * 4 + c] = ((acc + div / 2) / div) as u8;
                acc += at(x as isize + r as isize + 1);
                acc -= at(x as isize - r as isize);
            }
        }
    }
}

fn transpose(src: &[u8], dst: &mut [u8], w: usize, h: usize) {
    for y in 0..h {
        for x in 0..w {
            let s = (y * w + x) * 4;
            let d = (x * h + y) * 4;
            dst[d..d + 4].copy_from_slice(&src[s..s + 4]);
        }
    }
}

/// Blurs `data` (w×h premultiplied RGBA8) in place.
pub fn gaussian_blur(data: &mut [u8], w: usize, h: usize, sigma: f64) {
    if sigma < 0.3 || w == 0 || h == 0 {
        return;
    }
    let mut tmp = vec![0u8; data.len()];
    for b in boxes(sigma) {
        let r = (b - 1) / 2;
        if r == 0 {
            continue;
        }
        box_h(data, &mut tmp, w, h, r);
        transpose(&tmp, data, w, h);
        box_h(data, &mut tmp, h, w, r);
        transpose(&tmp, data, h, w);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preserves_energy_and_spreads() {
        let (w, h) = (41, 41);
        let mut img = vec![0u8; w * h * 4];
        let c = (20 * w + 20) * 4;
        img[c..c + 4].copy_from_slice(&[255, 255, 255, 255]);
        // A 9×9 white block so rounding does not erase the energy.
        for y in 16..25 {
            for x in 16..25 {
                let i = (y * w + x) * 4;
                img[i..i + 4].copy_from_slice(&[255, 255, 255, 255]);
            }
        }
        let before: u64 = img.iter().skip(3).step_by(4).map(|&a| u64::from(a)).sum();
        gaussian_blur(&mut img, w, h, 3.0);
        let after: u64 = img.iter().skip(3).step_by(4).map(|&a| u64::from(a)).sum();
        assert!(
            (before as f64 - after as f64).abs() / (before as f64) < 0.03,
            "{before} {after}"
        );
        assert!(img[(20 * w + 12) * 4 + 3] > 0, "spread outward");
        assert!(img[c + 3] < 255, "peak lowered");
    }

    #[test]
    fn small_sigma_is_noop() {
        let mut img = vec![7u8; 16];
        gaussian_blur(&mut img, 2, 2, 0.1);
        assert_eq!(img, vec![7u8; 16]);
    }
}
