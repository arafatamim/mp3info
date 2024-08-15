use std::{
    borrow::Cow,
    error::Error,
    fmt::{self, Debug, Display},
    io::{self, BufRead, Read},
};

use encoding::{
    all::{ISO_8859_1, UTF_16BE, UTF_16LE, UTF_8},
    DecoderTrap, Encoding as EncodingLib,
};

use crate::AppError;

#[allow(non_camel_case_types)]
#[derive(Debug, Clone, Copy)]
pub enum Encoding {
    ISO_8859_1 = 0x00,
    UTF_16 = 0x01,
    UTF_16BE = 0x02,
    UTF_8 = 0x03,
}

impl TryFrom<u8> for Encoding {
    type Error = Box<AppError>;
    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(Encoding::ISO_8859_1),
            1 => Ok(Encoding::UTF_16),
            2 => Ok(Encoding::UTF_16BE),
            3 => Ok(Encoding::UTF_8),
            _ => Err(AppError::new("parsing encoding from byte failed")),
        }
    }
}

#[derive(Debug)]
pub enum Content {
    Text(String),
    Binary(Vec<u8>),
}

impl From<Vec<u8>> for Content {
    fn from(v: Vec<u8>) -> Self {
        Self::Binary(v)
    }
}

#[derive(Debug, Clone, Copy, clap::ValueEnum, PartialEq, Eq)]
pub enum PictureType {
    Other = 0,
    Icon = 1,
    IconOther = 2,
    CoverFront = 3,
    CoverBack = 4,
    Leaflet = 5,
    Media = 6,
    LeadArtist = 7,
    Artist = 8,
    Conductor = 9,
    Band = 10,
    Composer = 11,
    Lyricist = 12,
    RecordingLocation = 13,
    DuringRecording = 14,
    DuringPerformance = 15,
    ScreenCapture = 16,
    BrightFish = 17,
    Illustration = 18,
    BandLogo = 19,
    PublisherLogo = 20,
}

impl TryFrom<u8> for PictureType {
    type Error = ();
    fn try_from(value: u8) -> Result<Self, Self::Error> {
        unsafe { std::mem::transmute(value) }
    }
}

#[derive(Debug)]
pub enum Frame {
    /// Unsynchronised lyrics/text transcription
    Uslt {
        text: String,
        language: String,
        description: String,
    },
    /// Attached picture
    Apic {
        data: Vec<u8>,
        picture_type: PictureType,
        description: String,
    },
    Other {
        id: String,
        content: Content,
    },
}

impl Display for Frame {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}",
            match self {
                Frame::Other { content, .. } => {
                    match content {
                        Content::Text(txt) => txt,
                        Content::Binary(_) => "(binary data)",
                    }
                }
                Frame::Uslt { text, .. } => {
                    text
                }
                Frame::Apic { .. } => "(pic)",
            }
        )
    }
}

#[derive(Debug)]
pub struct Header {
    pub version: u8,
    pub revision: u8,
    pub unsynchronisation: bool,
    pub extended: bool,
    pub experimental: bool,
    pub footer_present: bool,
    pub size: u32,
}

#[derive(Debug)]
pub struct Tag {
    pub header: Header,
    pub frames: Vec<Frame>,
}

pub(crate) fn is_bit_set(flag: u8, index: u8) -> bool {
    flag & (1 << index) != 0
}

pub(crate) fn byte_int(buf: &[u8]) -> u32 {
    u32::from_be_bytes(buf.try_into().unwrap())
}

pub(crate) fn byte_int_unsynch(buf: &[u8]) -> u32 {
    let be_int = byte_int(buf);
    be_int & 0xFF | (be_int & 0xFF00) >> 1 | (be_int & 0xFF_0000) >> 2 | (be_int & 0xFF00_0000) >> 3
}

pub(crate) fn consume_bytes(buf: &mut impl Read, size: usize) -> io::Result<Vec<u8>> {
    let mut b = vec![0; size];
    buf.read_exact(&mut b)?;
    Ok(b)
}

pub(crate) fn consume_c_str_bytes(buf: &mut impl BufRead) -> io::Result<Vec<u8>> {
    let mut b = Vec::new();
    buf.read_until(0x0, &mut b)?;
    Ok(b)
}

fn consume_c_str(buf: &mut impl BufRead) -> io::Result<String> {
    let b = consume_c_str_bytes(buf)?;
    decode_str(&b, Encoding::UTF_8)
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "couldn't decode utf8 string"))
}

fn consume_null_terminated_str_bytes(
    buf: &mut impl BufRead,
    encoding: Encoding,
) -> io::Result<Vec<u8>> {
    let res = match encoding {
        Encoding::UTF_8 | Encoding::ISO_8859_1 => consume_c_str_bytes(buf)?,
        Encoding::UTF_16 | Encoding::UTF_16BE => consume_utf16_str_bytes(buf),
    };
    Ok(res)
}

pub(crate) fn decode_str(buf: &[u8], encoding: Encoding) -> Result<String, Cow<'static, str>> {
    match encoding {
        Encoding::UTF_8 => UTF_8.decode(buf, DecoderTrap::Strict),
        Encoding::UTF_16 => UTF_16LE.decode(buf, DecoderTrap::Strict),
        Encoding::UTF_16BE => UTF_16BE.decode(buf, DecoderTrap::Strict),
        Encoding::ISO_8859_1 => ISO_8859_1.decode(buf, DecoderTrap::Strict),
    }
}

fn consume_utf16_str_bytes(buf: &mut impl Read) -> Vec<u8> {
    let mut strbuf: Vec<u8> = Vec::new();

    let mut last_byte = None;
    for b in buf.bytes() {
        let b = b.unwrap();
        match last_byte {
            Some(last_byte) if strbuf.len() % 2 != 0 && last_byte == 0x0 && b == 0x0 => {
                strbuf.push(b);
                break;
            }
            _ => {
                strbuf.push(b);
            }
        }
        last_byte = Some(b);
    }

    strbuf
}

pub(crate) fn read_text_from_buf(
    buf: &mut impl Read,
    size: usize,
    encoding: Encoding,
) -> Result<String, Cow<'static, str>> {
    let b = consume_bytes(buf, size).expect("couldn't consume bytes");
    decode_str(&b, encoding)
}

pub fn decode_header(buf: [u8; 10]) -> Result<Header, Box<dyn Error>> {
    let magic_str = String::from_utf8(buf[0..3].into())?;
    match magic_str {
        ref str if str == "ID3" => {}
        _ => return Err(AppError::new("ID3 tag should be at the start of file!")),
    };

    let version = buf[3];

    let revision = buf[4];

    let flag = buf[5];
    let unsynchronisation = is_bit_set(flag, 7);
    let extended = is_bit_set(flag, 6);
    let experimental = is_bit_set(flag, 5);
    let footer_present = is_bit_set(flag, 4);

    let size = byte_int_unsynch(&buf[6..10]);

    let header = Header {
        version,
        revision,
        unsynchronisation,
        extended,
        experimental,
        footer_present,
        size,
    };

    Ok(header)
}

pub fn decode_extended_header(_buf: Vec<u8>) -> Result<(), Box<dyn Error>> {
    todo!();
}

pub fn decode_frames(buf: Vec<u8>, v4: bool) -> Result<Vec<Frame>, Box<dyn Error>> {
    let mut buf = io::Cursor::new(buf);
    let mut frames: Vec<Frame> = Vec::new();

    loop {
        let id = {
            let b = consume_bytes(&mut buf, 4)?;
            String::from_utf8(b).unwrap_or("INVALID".into())
        };

        let size = {
            let b = consume_bytes(&mut buf, 4)?;
            if v4 {
                byte_int_unsynch(&b) as usize
            } else {
                byte_int(&b) as usize
            }
        };

        let _flags = consume_bytes(&mut buf, 2)?; // TODO: actually parse flags

        let encoding = {
            match id.as_str() {
                "RVAD" | "RVA2" | "SYLT" => Encoding::UTF_8,
                _ => {
                    let b = consume_bytes(&mut buf, 1)?;
                    Encoding::try_from(b[0])?
                }
            }
        };

        let size = if size > 0 { size - 1 } else { size }; // minus 1 byte for encoding;

        let frame = match id.as_str() {
            "TXXX" => {
                let description_bytes = consume_null_terminated_str_bytes(&mut buf, encoding)?;
                let description = decode_str(&description_bytes, encoding)?;
                let value = {
                    let b = consume_bytes(&mut buf, size - description_bytes.len())?;
                    decode_str(&b, encoding)?
                };
                Frame::Other {
                    id,
                    content: Content::Text(format!("{description}{value}")),
                }
            }
            "USLT" => {
                let language = {
                    let b = consume_bytes(&mut buf, 3)?;
                    decode_str(&b, Encoding::UTF_8)?
                };

                let description_bytes = consume_null_terminated_str_bytes(&mut buf, encoding)?;
                let description = decode_str(&description_bytes, encoding)?;

                let value = {
                    let b = consume_bytes(
                        &mut buf,
                        (size + 1)
                            - (1 // encoding bytes
                            + 3 // language bytes
                            + description_bytes.len()),
                    )?;
                    decode_str(&b, encoding)?
                };

                Frame::Uslt {
                    text: value,
                    language,
                    description,
                }
            }
            "COMM" => {
                let _language = {
                    let b = consume_bytes(&mut buf, 3)?;
                    decode_str(&b, Encoding::UTF_8)?
                };

                let description_bytes = match encoding {
                    Encoding::UTF_8 | Encoding::ISO_8859_1 => consume_c_str_bytes(&mut buf)?,
                    Encoding::UTF_16 | Encoding::UTF_16BE => consume_utf16_str_bytes(&mut buf),
                };

                let _description = decode_str(&description_bytes, encoding)?;

                let value = {
                    let b = consume_bytes(
                        &mut buf,
                        (size + 1)
                            - (1 // encoding bytes
                            + 3 // language bytes
                            + description_bytes.len()),
                    )?;
                    decode_str(&b, encoding)?
                };

                Frame::Other {
                    id,
                    content: Content::Text(value),
                }
            }
            "APIC" => {
                let mime_type = consume_c_str(&mut buf)?;
                let picture_type = consume_bytes(&mut buf, 1)?[0];

                let description_bytes = match encoding {
                    Encoding::UTF_8 | Encoding::ISO_8859_1 => consume_c_str_bytes(&mut buf)?,
                    Encoding::UTF_16 | Encoding::UTF_16BE => consume_utf16_str_bytes(&mut buf),
                };

                let description = decode_str(&description_bytes, encoding)?;

                let picture = consume_bytes(
                    &mut buf,
                    (size + 1)
                        - (2 // 1 byte for encoding & picture type each
                        + mime_type.len() + description_bytes.len()),
                )?;

                Frame::Apic {
                    data: picture,
                    description,
                    picture_type: picture_type.try_into().unwrap(), // unsafe code
                }
            }
            "RVAD" | "RVA2" => {
                let b = consume_bytes(&mut buf, size + 1)?; // discard the additional byte for now
                Frame::Other {
                    id,
                    content: Content::Binary(b),
                }
            }
            "SYLT" => {
                let b = consume_bytes(&mut buf, size + 1)?; // encoding not present in SYLT, so size is +1
                Frame::Other {
                    id,
                    content: Content::Binary(b),
                }
            }
            _ => {
                let text = read_text_from_buf(&mut buf, size, encoding)?;

                Frame::Other {
                    id,
                    content: Content::Text(text),
                }
            }
        };

        frames.push(frame);

        {
            let cur_pos = buf.position();
            let bytes = match consume_bytes(&mut buf, 4) {
                Ok(bytes) => bytes,
                Err(_) => break,
            };
            let num = byte_int(&bytes) as usize;
            if num == 0 {
                break;
            }
            buf.set_position(cur_pos);
        }
    }

    Ok(frames)
}

#[cfg(test)]
mod tests {
    use std::io::Read;

    #[test]
    fn parse_utf16_bytes() {
        let mut buf = std::io::Cursor::new([
            0xff, 0xfe, 0x43, 0x00, 0x6f, 0x00, 0x76, 0x00, 0x65, 0x00, 0x72, 0x00, 0x00, 0x00,
        ]);
        let bytes =
            super::consume_null_terminated_str_bytes(&mut buf, super::Encoding::UTF_16).unwrap();

        assert_eq!(
            &bytes,
            &[0xff, 0xfe, 0x43, 0x00, 0x6f, 0x00, 0x76, 0x00, 0x65, 0x00, 0x72, 0x00, 0x00, 0x00]
        );
        assert!(buf.bytes().next().is_none());
    }
}
