//! Prints how the GIF decoder reads a file's frames: `cargo run --example frames -- <file.gif>`.
use std::io::Cursor;

use image::AnimationDecoder;
use image::codecs::gif::GifDecoder;

fn main() {
    let path = std::env::args().nth(1).expect("a GIF");
    let bytes = std::fs::read(&path).unwrap();
    let decoder = GifDecoder::new(Cursor::new(&bytes)).unwrap();
    let mut count = 0;
    for frame in decoder.into_frames() {
        match frame {
            Ok(frame) => {
                count += 1;
                if count <= 3 {
                    println!("frame {count}: {:?} delay {:?}", frame.buffer().dimensions(), frame.delay().numer_denom_ms());
                }
            }
            Err(error) => {
                println!("error after {count} frames: {error}");
                break;
            }
        }
    }
    println!("{count} frames");
}
