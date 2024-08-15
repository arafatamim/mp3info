use clap::{Parser, Subcommand, ValueEnum};
use std::{
    error::Error,
    fmt::{self},
    fs,
    io::{Read, Write},
};

mod parser;

use parser::*;

#[derive(Debug)]
pub struct AppError {
    details: String,
}

impl AppError {
    pub fn new(msg: &str) -> Box<Self> {
        Box::new(AppError {
            details: msg.into(),
        })
    }
}

impl Error for AppError {}

impl fmt::Display for AppError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.details)
    }
}

#[derive(Parser)]
#[command(version)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Display commonly used song metadata
    Info {
        path: String,
    },
    /// View song lyrics
    Lyrics {
        path: String,
    },
    /// Emit picture as binary data
    Picture {
        path: String,
        #[arg(long, short = 't', default_value_t = PictureType::CoverFront, value_enum)]
        picture_type: PictureType,
        #[arg(short = 'l', long, default_value = "false")]
        list: bool,
    },
}

fn read_file(path: &str) -> Result<Tag, Box<dyn Error>> {
    let mut file = fs::File::open(path)?;

    let tag_headers = {
        let mut tag_headers = [0; 10];
        file.read_exact(&mut tag_headers)?;
        tag_headers
    };

    let header = decode_header(tag_headers)?;

    if header.extended {
        let extended_header_size = consume_bytes(&mut file, 4)?;
        let extended_header_size = byte_int(&extended_header_size);

        let mut extended_header_data = vec![0; extended_header_size as usize - 4]; // minus 4 bytes for the size field
        file.read_exact(&mut extended_header_data)?; // consume extended header data
    }

    let tag_frames = {
        let mut tag_frames = vec![0; header.size as usize];
        file.read_exact(&mut tag_frames)?;
        tag_frames
    };

    let frames = decode_frames(tag_frames, header.version == 4)?;

    Ok(Tag { header, frames })
}

fn find_frame_by_id<'a>(f: &'a [Frame], id: &str) -> Option<&'a Frame> {
    for frame in f {
        match frame {
            Frame::Other { id: tid, .. } => {
                if id == tid {
                    return Some(frame);
                }
            }
            _ => continue,
        }
    }
    None
}

fn main() -> Result<(), Box<dyn Error>> {
    let cli = Cli::parse();
    match cli.command {
        Commands::Info { path } => {
            let tag = read_file(&path)?;

            let title = find_frame_by_id(&tag.frames, "TIT2");
            let lead_artist = find_frame_by_id(&tag.frames, "TPE1");
            let album = find_frame_by_id(&tag.frames, "TALB");
            let band = find_frame_by_id(&tag.frames, "TPE2");
            let year = find_frame_by_id(&tag.frames, "TYER");
            let comment = find_frame_by_id(&tag.frames, "COMM");

            if let Some(x) = title {
                println!("Title: {}", x);
            }
            if let Some(x) = lead_artist {
                println!("Lead performer: {}", x);
            }
            if let Some(x) = album {
                println!("Album: {}", x);
            }
            if let Some(x) = year {
                println!("Year: {}", x);
            }
            if let Some(x) = band {
                println!("Band: {}", x);
            }
            if let Some(x) = comment {
                println!("Comment: {}", x);
            }
        }
        Commands::Lyrics { path } => {
            let tag = read_file(&path)?;
            let frames = tag.frames;

            if !frames.iter().any(|x| matches!(&x, Frame::Uslt { .. })) {
                return Err(AppError::new("Lyrics not available").into());
            }

            for frame in frames {
                match frame {
                    Frame::Uslt {
                        text,
                        language,
                        description,
                    } => {
                        println!("Language: {}", language);
                        println!("Description: {}", description.trim());
                        println!("=== \n{}", text);
                    }
                    _ => continue,
                }
                println!();
            }
        }
        Commands::Picture {
            path,
            picture_type,
            list,
        } => {
            let tag = read_file(&path)?;
            let mut frames_iter = tag.frames.iter();

            if list {
                let pics = frames_iter.filter_map(|x| match x {
                    Frame::Apic {
                        picture_type: ptype,
                        ..
                    } => Some(ptype),
                    _ => None,
                });

                for pic in pics {
                    println!("{}", pic.to_possible_value().unwrap().get_name());
                }
                return Ok(());
            }

            let pic = frames_iter.find(|x| {
                matches!(x, Frame::Apic {
                    picture_type: ptype,
                    ..
                } if &picture_type == ptype)
            });

            match pic {
                Some(Frame::Apic { data, .. }) => {
                    eprintln!("Picture length: {}", data.len());
                    let mut handle = std::io::stdout().lock();
                    if atty::is(atty::Stream::Stdout) {
                        println!(
                            "Binary output not displayed! Pipe stdout into a file to save it."
                        );
                    } else {
                        handle.write_all(data)?;
                    }
                    handle.flush()?
                }
                _ => {
                    return Err(AppError::new(&format!(
                        "Attached picture type '{}' not available",
                        picture_type.to_possible_value().unwrap().get_name()
                    ))
                    .into());
                }
            }
        }
    }

    Ok(())
}
