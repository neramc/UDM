//! Parsers for `/proc/self/mountinfo` and `/proc/swaps`.
//!
//! Both parsers are pure functions over text so they can be tested without a
//! real `/proc`.

use std::collections::HashMap;

/// One mounted filesystem, keyed by the `major:minor` of its block device.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct MountEntry {
    pub(crate) mount_point: String,
    pub(crate) filesystem: String,
}

/// Mounts grouped by device number (`major:minor`).
pub(crate) type MountTable = HashMap<String, Vec<MountEntry>>;

/// Parses `/proc/self/mountinfo`.
///
/// Format (see proc(5)):
/// `id parent major:minor root mount-point options [optional...] - fstype source super-options`
///
/// Malformed lines are skipped and counted; the count is returned so the
/// caller can report it instead of silently ignoring it.
pub(crate) fn parse_mountinfo(text: &str) -> (MountTable, usize) {
    let mut table: MountTable = HashMap::new();
    let mut malformed = 0;
    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        let fields: Vec<&str> = line.split(' ').collect();
        let separator = fields.iter().position(|f| *f == "-");
        let (Some(dev), Some(mount_point), Some(sep)) = (fields.get(2), fields.get(4), separator)
        else {
            malformed += 1;
            continue;
        };
        let Some(filesystem) = fields.get(sep + 1) else {
            malformed += 1;
            continue;
        };
        if sep < 6 || !dev.contains(':') {
            malformed += 1;
            continue;
        }
        table
            .entry((*dev).to_owned())
            .or_default()
            .push(MountEntry {
                mount_point: unescape(mount_point),
                filesystem: (*filesystem).to_owned(),
            });
    }
    (table, malformed)
}

/// Parses `/proc/swaps` and returns the swap device paths (first column).
/// Swap *files* are returned too; callers only match block device names.
pub(crate) fn parse_swaps(text: &str) -> Vec<String> {
    text.lines()
        .skip(1)
        .filter_map(|line| line.split_whitespace().next())
        .map(unescape)
        .collect()
}

/// Decodes the kernel's octal escapes (`\040` for space, etc.).
pub(crate) fn unescape(field: &str) -> String {
    let bytes = field.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'\\' && i + 4 <= bytes.len() {
            let digits = &bytes[i + 1..i + 4];
            if digits.iter().all(|d| (b'0'..=b'7').contains(d)) {
                let value = digits
                    .iter()
                    .fold(0u32, |acc, d| acc * 8 + u32::from(d - b'0'));
                if let Ok(byte) = u8::try_from(value) {
                    out.push(byte);
                    i += 4;
                    continue;
                }
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "\
23 28 0:22 / /proc rw,relatime - proc proc rw
28 1 254:0 / / rw,relatime - ext4 /dev/vda rw,discard
31 28 8:17 / /media/user/MY\\040STICK rw,nosuid shared:7 master:1 - vfat /dev/sdb1 rw
32 28 8:17 / /mnt/bind rw - vfat /dev/sdb1 rw
garbage line
";

    #[test]
    fn parses_mountinfo_with_optional_fields_and_escapes() {
        let (table, malformed) = parse_mountinfo(SAMPLE);
        assert_eq!(malformed, 1);
        assert_eq!(
            table["254:0"],
            vec![MountEntry {
                mount_point: "/".into(),
                filesystem: "ext4".into()
            }]
        );
        let stick = &table["8:17"];
        assert_eq!(stick.len(), 2);
        assert_eq!(stick[0].mount_point, "/media/user/MY STICK");
        assert_eq!(stick[0].filesystem, "vfat");
    }

    #[test]
    fn parses_swaps() {
        let text = "Filename\tType\tSize\tUsed\tPriority\n/dev/sda2 partition 1 0 -2\n/swap\\040file file 1 0 -3\n";
        assert_eq!(parse_swaps(text), vec!["/dev/sda2", "/swap file"]);
        assert!(parse_swaps("Filename Type Size Used Priority\n").is_empty());
    }

    #[test]
    fn unescape_handles_edge_cases() {
        assert_eq!(unescape("a\\040b"), "a b");
        assert_eq!(unescape("a\\134b"), "a\\b");
        assert_eq!(unescape("trailing\\"), "trailing\\");
        assert_eq!(unescape("short\\04"), "short\\04");
        assert_eq!(unescape("bad\\089"), "bad\\089");
        assert_eq!(unescape("한글\\040이름"), "한글 이름");
    }
}
