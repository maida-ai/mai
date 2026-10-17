//! Lossless platform-tagged native paths for JSON. These are local identities,
//! not portable paths: a decoder rejects another platform's representation.
use anyhow::{Result, ensure};
use base64::{Engine, engine::general_purpose::STANDARD};
use serde::{Deserialize, Deserializer, Serialize, Serializer, de::Error};
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NativePath(PathBuf);

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct EncodedPath {
    encoding: Encoding,
    data: String,
}
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum Encoding {
    UnixBytes,
    WindowsUtf16le,
}

impl NativePath {
    pub fn from_path(path: &Path) -> Result<Self> {
        let bytes = native_bytes(path);
        decode_native(&bytes)?;
        Ok(Self(path.to_owned()))
    }
    pub fn as_path(&self) -> &Path {
        &self.0
    }
}
impl Serialize for NativePath {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        EncodedPath {
            encoding: native_encoding(),
            data: STANDARD.encode(native_bytes(&self.0)),
        }
        .serialize(serializer)
    }
}
impl<'de> Deserialize<'de> for NativePath {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let encoded = EncodedPath::deserialize(deserializer)?;
        ensure_native_encoding(encoded.encoding).map_err(D::Error::custom)?;
        let bytes = STANDARD.decode(&encoded.data).map_err(D::Error::custom)?;
        if STANDARD.encode(&bytes) != encoded.data {
            return Err(D::Error::custom(
                "native path requires canonical padded base64",
            ));
        }
        decode_native(&bytes).map(Self).map_err(D::Error::custom)
    }
}

#[cfg(unix)]
fn native_encoding() -> Encoding {
    Encoding::UnixBytes
}
#[cfg(windows)]
fn native_encoding() -> Encoding {
    Encoding::WindowsUtf16le
}
fn ensure_native_encoding(encoding: Encoding) -> Result<()> {
    #[cfg(unix)]
    ensure!(
        matches!(encoding, Encoding::UnixBytes),
        "foreign native path encoding"
    );
    #[cfg(windows)]
    ensure!(
        matches!(encoding, Encoding::WindowsUtf16le),
        "foreign native path encoding"
    );
    Ok(())
}
#[cfg(unix)]
fn native_bytes(path: &Path) -> Vec<u8> {
    use std::os::unix::ffi::OsStrExt;
    path.as_os_str().as_bytes().to_vec()
}
#[cfg(windows)]
fn native_bytes(path: &Path) -> Vec<u8> {
    use std::os::windows::ffi::OsStrExt;
    path.as_os_str()
        .encode_wide()
        .flat_map(u16::to_le_bytes)
        .collect()
}
#[cfg(unix)]
fn decode_native(bytes: &[u8]) -> Result<PathBuf> {
    use std::{ffi::OsString, os::unix::ffi::OsStringExt};
    ensure!(!bytes.contains(&0), "NUL in native path");
    Ok(OsString::from_vec(bytes.to_vec()).into())
}
#[cfg(windows)]
fn decode_native(bytes: &[u8]) -> Result<PathBuf> {
    use std::{ffi::OsString, os::windows::ffi::OsStringExt};
    ensure!(
        bytes.len() % 2 == 0,
        "odd byte count in UTF-16LE native path"
    );
    let units: Vec<_> = bytes
        .chunks_exact(2)
        .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
        .collect();
    ensure!(!units.contains(&0), "NUL in native path");
    Ok(OsString::from_wide(&units).into())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn native_path_round_trip_is_explicit_and_strict() {
        let path = NativePath::from_path(Path::new("space-é")).unwrap();
        let json = serde_json::to_value(&path).unwrap();
        assert_eq!(
            serde_json::from_value::<NativePath>(json.clone()).unwrap(),
            path
        );
        assert!(json["encoding"].is_string());
        for malformed in [
            serde_json::json!("ordinary path"),
            serde_json::json!({"encoding":"unknown", "data":""}),
            serde_json::json!({"encoding":json["encoding"], "data":"not base64!"}),
            serde_json::json!({"encoding":json["encoding"], "data":"Zg"}),
            serde_json::json!({"encoding":json["encoding"], "data":"Zh=="}),
            serde_json::json!({"encoding":json["encoding"], "data":"AAA="}),
            serde_json::json!({"encoding":json["encoding"], "data":"", "extra":1}),
        ] {
            assert!(serde_json::from_value::<NativePath>(malformed).is_err());
        }
        #[cfg(unix)]
        let foreign = "windows_utf16le";
        #[cfg(windows)]
        let foreign = "unix_bytes";
        assert!(
            serde_json::from_value::<NativePath>(serde_json::json!({"encoding":foreign,"data":""}))
                .is_err()
        );
    }
    #[cfg(unix)]
    #[test]
    fn unix_bytes_preserve_invalid_utf8_without_replacement() {
        use std::{ffi::OsString, os::unix::ffi::OsStringExt};
        let path = PathBuf::from(OsString::from_vec(b"/tmp/\xff\xfe".to_vec()));
        let native = NativePath::from_path(&path).unwrap();
        let encoded = serde_json::to_value(&native).unwrap();
        assert_eq!(encoded["encoding"], "unix_bytes");
        assert_eq!(encoded["data"], STANDARD.encode(b"/tmp/\xff\xfe"));
        assert_eq!(
            serde_json::from_value::<NativePath>(encoded)
                .unwrap()
                .as_path(),
            path
        );
        assert!(NativePath::from_path(&PathBuf::from(OsString::from_vec(vec![0]))).is_err());
    }
    #[cfg(windows)]
    #[test]
    fn windows_units_preserve_pairs_and_unpaired_surrogates() {
        use std::{ffi::OsString, os::windows::ffi::OsStringExt};
        let units = [
            b'C' as u16,
            b':' as u16,
            b'\\' as u16,
            0xd83d,
            0xde00,
            0xd800,
        ];
        let path = PathBuf::from(OsString::from_wide(&units));
        let native = NativePath::from_path(&path).unwrap();
        let encoded = serde_json::to_value(&native).unwrap();
        assert_eq!(encoded["encoding"], "windows_utf16le");
        assert_eq!(
            encoded["data"],
            STANDARD.encode(
                units
                    .into_iter()
                    .flat_map(u16::to_le_bytes)
                    .collect::<Vec<_>>()
            )
        );
        assert_eq!(
            serde_json::from_value::<NativePath>(encoded)
                .unwrap()
                .as_path(),
            path
        );
        assert!(NativePath::from_path(&PathBuf::from(OsString::from_wide(&[0]))).is_err());
        assert!(
            serde_json::from_value::<NativePath>(
                serde_json::json!({"encoding":"windows_utf16le","data":"YQ=="})
            )
            .is_err()
        );
    }
}
