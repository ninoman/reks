//! Probe, tag and artwork tests on the generated fixtures (`fixtures/audio/`).

use std::path::PathBuf;

use lofty::config::WriteOptions;
use lofty::file::{AudioFile, TaggedFileExt};
use lofty::picture::{MimeType, Picture as LoftyPicture, PictureType};
use lofty::tag::Tag;
use reks_audio::{embedded_artwork, probe, tags, Error, Kind};

/// A valid 1×1 RGBA PNG.
const PNG_1X1: &[u8] = &[
    0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x48, 0x44, 0x52,
    0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00, 0x00, 0x1F, 0x15, 0xC4,
    0x89, 0x00, 0x00, 0x00, 0x0A, 0x49, 0x44, 0x41, 0x54, 0x78, 0x9C, 0x63, 0x00, 0x01, 0x00, 0x00,
    0x05, 0x00, 0x01, 0x0D, 0x0A, 0x2D, 0xB4, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45, 0x4E, 0x44, 0xAE,
    0x42, 0x60, 0x82,
];

fn fixture(ext: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/audio")
        .join(format!("sine-click-128.{ext}"))
}

#[test]
fn probe_every_fixture() {
    // (ext, kind, bit depth, duration ms, bit rate kbps range).
    // Fixture: 30 s, 44.1 kHz, mono, 16-bit. The MP3's LAME header lets
    // Symphonia trim encoder delay/padding; AAC priming is not trimmed
    // (Symphonia has no AAC gapless), so the M4A reads ~23 ms long.
    let cases = [
        ("wav", Kind::Wav, Some(16), 30_000, 705..=706),
        ("aiff", Kind::Aiff, Some(16), 30_000, 705..=706),
        ("flac", Kind::Flac, Some(16), 30_000, 100..=200),
        ("mp3", Kind::Mp3, None, 30_000, 63..=65),
        ("m4a", Kind::Aac, None, 30_023, 55..=70),
    ];
    for (ext, kind, bit_depth, duration_ms, kbps) in cases {
        let p = probe(&fixture(ext)).unwrap();
        assert_eq!(p.kind, kind, "{ext}");
        assert_eq!(p.sample_rate, 44_100, "{ext}");
        assert_eq!(p.channels, 1, "{ext}");
        assert_eq!(p.bit_depth, bit_depth, "{ext}");
        assert_eq!(p.duration_ms, duration_ms, "{ext}");
        let rate = p.bit_rate_kbps.unwrap_or(0);
        assert!(kbps.contains(&rate), "{ext}: bit rate {rate}");
    }
}

#[test]
fn probe_rejects_non_audio() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("notes.mp3");
    std::fs::write(&path, b"definitely not audio").unwrap();
    let err = probe(&path).unwrap_err();
    assert!(
        matches!(err, Error::Unsupported { .. } | Error::Decode { .. }),
        "{err:?}"
    );
    assert!(matches!(
        probe(&dir.path().join("missing.wav")).unwrap_err(),
        Error::Io { .. }
    ));
}

#[test]
fn tags_from_fixtures() {
    for ext in ["mp3", "flac", "m4a"] {
        let t = tags(&fixture(ext)).unwrap();
        assert_eq!(t.title.as_deref(), Some("Sine Click 128"), "{ext}");
        assert_eq!(t.artist.as_deref(), Some("reks fixtures"), "{ext}");
    }
    // ffmpeg's AIFF muxer keeps only the title (fixtures/README.md).
    let aiff = tags(&fixture("aiff")).unwrap();
    assert_eq!(aiff.title.as_deref(), Some("Sine Click 128"));
    assert_eq!(aiff.artist, None);
    assert_eq!(tags(&fixture("wav")).unwrap(), Default::default());
}

#[test]
fn fixtures_have_no_artwork() {
    for ext in ["wav", "aiff", "flac", "mp3", "m4a"] {
        assert_eq!(embedded_artwork(&fixture(ext)).unwrap(), None, "{ext}");
    }
}

#[test]
fn embedded_artwork_prefers_front_cover() {
    let dir = tempfile::tempdir().unwrap();
    for ext in ["mp3", "flac", "m4a"] {
        let path = dir.path().join(format!("art.{ext}"));
        std::fs::copy(fixture(ext), &path).unwrap();

        let mut file = lofty::read_from_path(&path).unwrap();
        if file.primary_tag().is_none() {
            file.insert_tag(Tag::new(file.primary_tag_type()));
        }
        let tag = file.primary_tag_mut().unwrap();
        // MP4 has no picture types; there the only picture is the cover.
        if ext != "m4a" {
            tag.push_picture(
                LoftyPicture::unchecked(b"not the cover".to_vec())
                    .pic_type(PictureType::Artist)
                    .mime_type(MimeType::Jpeg)
                    .build(),
            );
        }
        tag.push_picture(
            LoftyPicture::unchecked(PNG_1X1.to_vec())
                .pic_type(PictureType::CoverFront)
                .mime_type(MimeType::Png)
                .build(),
        );
        file.save_to_path(&path, WriteOptions::default()).unwrap();

        let pic = embedded_artwork(&path).unwrap().expect("picture");
        assert_eq!(pic.data, PNG_1X1, "{ext}");
        assert_eq!(pic.mime.as_deref(), Some("image/png"), "{ext}");
        // Writing art must not disturb the probe.
        assert_eq!(probe(&path).unwrap().sample_rate, 44_100);
    }
}
