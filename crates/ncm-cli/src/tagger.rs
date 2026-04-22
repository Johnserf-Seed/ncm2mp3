//! Write NCM metadata and cover art into the decrypted output file using
//! lofty, which abstracts over ID3v2 / Vorbis Comment / MP4 atoms.

use std::path::Path;

use anyhow::{Context, Result};
use lofty::config::WriteOptions;
use lofty::file::{AudioFile, TaggedFileExt};
use lofty::picture::{MimeType, Picture, PictureType};
use lofty::tag::{Accessor, Tag};
use ncm_core::{Cover, CoverMime, NcmMetadata};

pub fn write_tags(path: &Path, metadata: &NcmMetadata, cover: Option<&Cover>) -> Result<()> {
    let mut tagged = lofty::read_from_path(path)
        .with_context(|| format!("failed to read {} for tagging", path.display()))?;

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
        .save_to_path(path, WriteOptions::default())
        .with_context(|| format!("failed to write tags to {}", path.display()))?;

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
