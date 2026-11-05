//! Write NCM metadata and cover art into the decrypted output file using
//! lofty, which abstracts over ID3v2 / Vorbis Comment / MP4 atoms.

use std::fs::File;
use std::io::{BufReader, Seek};

use anyhow::{Context, Result};
use lofty::config::WriteOptions;
use lofty::file::{AudioFile, FileType, TaggedFileExt};
use lofty::picture::{MimeType, Picture, PictureType};
use lofty::probe::Probe;
use lofty::tag::{Accessor, Tag};
use ncm_core::{AudioFormat, Cover, CoverMime, NcmMetadata};

/// Tag the decrypted `format` audio in `file`, which must be open for
/// reading and writing. Takes the open handle rather than a path, so nothing
/// can swap the file for a symlink between decrypting and tagging it.
pub fn write_tags(
    file: &mut File,
    format: AudioFormat,
    metadata: &NcmMetadata,
    cover: Option<&Cover>,
) -> Result<()> {
    // `lofty::read_from_path` picks the parser from the file extension; the
    // extension we gave the file is `format`'s, so pick it from that.
    let file_type = FileType::from_ext(format.extension())
        .with_context(|| format!("can't tag .{} files", format.extension()))?;
    file.rewind()
        .context("failed to read the audio for tagging")?;
    let mut tagged = Probe::with_file_type(BufReader::new(&mut *file), file_type)
        .read()
        .context("failed to read the audio for tagging")?;

    let tag_type = tagged.primary_tag_type();
    if tagged.primary_tag().is_none() {
        tagged.insert_tag(Tag::new(tag_type));
    }
    let tag = tagged
        .primary_tag_mut()
        .expect("primary_tag just inserted above");

    if !metadata.title.is_empty() {
        tag.set_title(metadata.title.clone());
    }
    if !metadata.artists.is_empty() {
        tag.set_artist(metadata.artists_joined(" / "));
    }
    if !metadata.album.is_empty() {
        tag.set_album(metadata.album.clone());
    }

    if let Some(cover) = cover {
        if let Some(picture) = build_picture(cover) {
            tag.push_picture(picture);
        }
    }

    tagged
        .save_to(file, WriteOptions::default())
        .context("failed to write tags")?;

    Ok(())
}

fn build_picture(cover: &Cover) -> Option<Picture> {
    let mime = match cover.mime {
        CoverMime::Jpeg => MimeType::Jpeg,
        CoverMime::Png => MimeType::Png,
        CoverMime::Unknown => return None,
    };

    // lofty 0.24 replaced `Picture::new_unchecked(...)` with a builder
    // initiated by `Picture::unchecked(data)`.
    Some(
        Picture::unchecked(cover.data.clone())
            .pic_type(PictureType::CoverFront)
            .mime_type(mime)
            .build(),
    )
}
