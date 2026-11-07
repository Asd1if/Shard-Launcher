//! Уменьшение слишком больших иконок сборок. Иконка хранится прямо в instance.json,
//! поэтому многомегабайтный анимированный логотип модпака ужимаем, сохраняя анимацию.

use std::io::Cursor;

use image::codecs::gif::{GifDecoder, GifEncoder, Repeat};
use image::imageops::FilterType;
use image::{AnimationDecoder, Frame, ImageFormat};

use crate::error::{Error, Result};

/// Сторона анимированной иконки: в интерфейсе иконки не больше 64 точек, 128 — с запасом на HiDPI.
const GIF_SIDE: u32 = 128;
const STILL_SIDE: u32 = 256;

/// Возвращает (mime, байты) уменьшенной картинки.
pub fn shrink(mime: &str, bytes: &[u8]) -> Result<(String, Vec<u8>)> {
    if mime == "image/gif" {
        return Ok(("image/gif".into(), shrink_gif(bytes)?));
    }
    let img = image::load_from_memory(bytes).map_err(|e| Error::msg(e.to_string()))?;
    let small = img.resize(STILL_SIDE, STILL_SIDE, FilterType::Triangle);
    let mut out = Cursor::new(Vec::new());
    small.write_to(&mut out, ImageFormat::Png).map_err(|e| Error::msg(e.to_string()))?;
    Ok(("image/png".into(), out.into_inner()))
}

fn shrink_gif(bytes: &[u8]) -> Result<Vec<u8>> {
    let decoder = GifDecoder::new(Cursor::new(bytes)).map_err(|e| Error::msg(e.to_string()))?;
    let frames = decoder.into_frames().collect_frames().map_err(|e| Error::msg(e.to_string()))?;
    let mut out = Vec::new();
    {
        let mut encoder = GifEncoder::new_with_speed(&mut out, 10);
        encoder.set_repeat(Repeat::Infinite).map_err(|e| Error::msg(e.to_string()))?;
        for frame in frames {
            let delay = frame.delay();
            let small = image::imageops::resize(frame.buffer(), GIF_SIDE, GIF_SIDE, FilterType::Triangle);
            encoder.encode_frame(Frame::from_parts(small, 0, 0, delay)).map_err(|e| Error::msg(e.to_string()))?;
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{Delay, RgbaImage};

    #[test]
    fn keeps_animation_when_shrinking() {
        let mut big = Vec::new();
        {
            let mut encoder = GifEncoder::new(&mut big);
            encoder.set_repeat(Repeat::Infinite).unwrap();
            for shade in [0u8, 120, 240] {
                let img = RgbaImage::from_pixel(512, 512, image::Rgba([shade, 50, 200, 255]));
                encoder.encode_frame(Frame::from_parts(img, 0, 0, Delay::from_numer_denom_ms(100, 1))).unwrap();
            }
        }
        let (mime, small) = shrink("image/gif", &big).unwrap();
        assert_eq!(mime, "image/gif");
        let frames = GifDecoder::new(Cursor::new(&small)).unwrap().into_frames().collect_frames().unwrap();
        assert_eq!(frames.len(), 3);
        assert_eq!(frames[0].buffer().dimensions(), (GIF_SIDE, GIF_SIDE));
    }
}
