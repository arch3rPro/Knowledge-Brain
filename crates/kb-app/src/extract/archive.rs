use std::{
    collections::BTreeMap,
    io::{Cursor, Read},
    path::Component,
};

const MAX_ENTRIES: usize = 4_096;
const MAX_ENTRY_BYTES: u64 = 16 * 1024 * 1024;
const MAX_EXPANDED_BYTES: u64 = 64 * 1024 * 1024;

pub(super) struct BoundedArchive {
    entries: BTreeMap<String, Vec<u8>>,
}

impl BoundedArchive {
    pub(super) fn open(bytes: &[u8], format: &str) -> Result<Self, String> {
        let mut archive = zip::ZipArchive::new(Cursor::new(bytes))
            .map_err(|error| format!("Cannot open {format} archive: {error}"))?;
        if archive.len() > MAX_ENTRIES {
            return Err(format!("{format} archive entry limit exceeded."));
        }
        let mut entries = BTreeMap::new();
        let mut expanded = 0_u64;
        for index in 0..archive.len() {
            let mut file = archive
                .by_index(index)
                .map_err(|error| format!("Cannot read {format} archive entry: {error}"))?;
            if file.encrypted() {
                return Err(format!(
                    "Encrypted {format} archive entries are unsupported."
                ));
            }
            if file.is_dir() {
                continue;
            }
            let name = safe_name(file.name(), format)?;
            if file.size() > MAX_ENTRY_BYTES {
                return Err(format!(
                    "{format} archive entry size limit exceeded: {name}"
                ));
            }
            expanded = expanded
                .checked_add(file.size())
                .ok_or_else(|| format!("{format} archive expansion limit exceeded."))?;
            if expanded > MAX_EXPANDED_BYTES {
                return Err(format!("{format} archive expansion limit exceeded."));
            }
            let capacity = usize::try_from(file.size())
                .map_err(|_| format!("{format} archive entry size does not fit this platform."))?;
            let mut data = Vec::with_capacity(capacity);
            file.by_ref()
                .take(MAX_ENTRY_BYTES + 1)
                .read_to_end(&mut data)
                .map_err(|error| format!("Cannot expand {format} archive entry {name}: {error}"))?;
            if data.len() as u64 > MAX_ENTRY_BYTES {
                return Err(format!(
                    "{format} archive entry size limit exceeded: {name}"
                ));
            }
            if entries.insert(name.clone(), data).is_some() {
                return Err(format!("{format} archive contains duplicate entry: {name}"));
            }
        }
        Ok(Self { entries })
    }

    pub(super) fn required(&self, name: &str, format: &str) -> Result<&[u8], String> {
        self.entries
            .get(name)
            .map(Vec::as_slice)
            .ok_or_else(|| format!("{format} archive is missing {name}."))
    }
}

pub(super) fn resolve(base_file: &str, reference: &str) -> Result<String, String> {
    let (path, fragment) = reference
        .split_once('#')
        .map_or((reference, None), |(path, fragment)| (path, Some(fragment)));
    if path.contains(':') || path.starts_with('/') || path.contains('\\') {
        return Ok(reference.to_owned());
    }
    let base = base_file.rsplit_once('/').map_or("", |(parent, _)| parent);
    let joined = if path.is_empty() {
        base_file.to_owned()
    } else if base.is_empty() {
        path.to_owned()
    } else {
        format!("{base}/{path}")
    };
    let normalized = normalize(&joined)?;
    Ok(fragment.map_or(normalized.clone(), |value| format!("{normalized}#{value}")))
}

fn safe_name(name: &str, format: &str) -> Result<String, String> {
    if name.is_empty() || name.starts_with('/') || name.contains('\\') {
        return Err(format!("{format} archive contains an unsafe entry name."));
    }
    normalize(name).map_err(|_| format!("{format} archive contains an unsafe entry name: {name}"))
}

fn normalize(path: &str) -> Result<String, String> {
    let mut parts = Vec::new();
    for component in std::path::Path::new(path).components() {
        match component {
            Component::Normal(value) => {
                let value = value
                    .to_str()
                    .ok_or_else(|| "Archive path is not UTF-8.".to_owned())?;
                parts.push(value);
            }
            Component::CurDir => {}
            Component::ParentDir => {
                parts
                    .pop()
                    .ok_or_else(|| "Archive path escapes its root.".to_owned())?;
            }
            Component::Prefix(_) | Component::RootDir => {
                return Err("Archive path must be relative.".to_owned());
            }
        }
    }
    if parts.is_empty() {
        return Err("Archive path is empty.".to_owned());
    }
    Ok(parts.join("/"))
}
