use fnv::FnvHashMap;
use itertools::Itertools;
use lazy_static::lazy_static;
use std::cmp::Ordering;
use std::fs::File;
use std::io::{BufRead, BufReader, Result as IoResult};
use std::os::unix::{ffi::OsStrExt, fs::PermissionsExt};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::borrow::Cow;
use walkdir::{DirEntry, WalkDir};

lazy_static! {
    static ref ICONS: FnvHashMap<String, String> = map_mime_icon_file("/usr/share/mime/icons");
    static ref GENERIC: FnvHashMap<String, String> = map_mime_icon_file("/usr/share/mime/generic-icons");
}

fn map_mime_icon_file(file: &str) -> FnvHashMap<String, String> {
        let file = File::open(file).expect("mime icons");
        let buf = BufReader::new(file);

        let split_line = |line: IoResult<String>| {
            let line = line.expect("parse line");
            let mut split = line.splitn(2, ':').map(str::to_string);
            (split.next().expect("mime type"), split.next().expect("icon name"))
        };

        buf.lines().map(split_line).collect::<FnvHashMap<String, String>>()
}

fn icon_from_mimetype(mime: &str) -> Cow<str> {
    ICONS.get(mime).or_else(|| GENERIC.get(mime)).map(Cow::from)
        .or_else(|| Some(Cow::from(format!("{}-x-generic", mime.split('/').nth(0)?))))
        .unwrap_or_else(|| Cow::from("unknown"))
}

fn generate_list<P: AsRef<Path>>(cursor: P) -> String {
    let dirs_first = |a: &DirEntry, b: &DirEntry| {
        match (a.file_type().is_dir(), b.file_type().is_dir()) {
            (true, false) => Ordering::Less,
            (false, true) => Ordering::Greater,
            (_,_) => a.file_name().cmp(b.file_name())
        }
    };

    let dotfiles = |entry: &DirEntry| {
        entry.file_name()
            .to_str()
            .map(|s| ! s.starts_with('.'))
            .unwrap_or(true)
    };

    let to_output = |entry: DirEntry| -> String {
        let mime = tree_magic::from_filepath(entry.path());
        let name = entry.file_name().to_string_lossy();
        let icon = icon_from_mimetype(&mime);
        format!("{}\x00icon\x1f{}", name, icon)
    };

    let out = WalkDir::new(cursor)
        .max_depth(1)
        .min_depth(1)
        .sort_by(dirs_first)
        .into_iter()
        .filter_entry(dotfiles)
        .filter_map(Result::ok)
        .map(to_output)
        .join("\n");

    format!("..\x00icon\x1ffolder\n{}", out)
}

fn main() -> IoResult<()> {
    let cache_dir = dirs::cache_dir().expect("cache dir").join("rofi");
    std::fs::create_dir_all(&cache_dir)?;

    let lastdir_path = cache_dir.join("rofi_file_lastdir");
    let lastdir = PathBuf::from(std::fs::read_to_string(&lastdir_path).unwrap_or_default());

    let mut cursor = {
        if lastdir.is_dir() {
            lastdir
        } else {
            dirs::home_dir().expect("cache_dir")
        }
    };

    if let Some(arg) = std::env::args().nth(1) {
        let new = PathBuf::from(arg);

        if new.is_absolute() {
            cursor = new;
        } else {
            cursor.push(new);
        }

        if cursor.is_file() {
            if std::fs::metadata(&cursor)?.permissions().mode() & 0o111 != 0 {
                Command::new(&cursor)
                    .stdin(Stdio::null())
                    .stdout(Stdio::null())
                    .stderr(Stdio::null())
                    .spawn()?;
            } else {
                Command::new("/usr/bin/gio")
                    .arg("open")
                    .arg(format!("file://{}", cursor.display()))
                    .stdin(Stdio::null())
                    .stdout(Stdio::null())
                    .stderr(Stdio::null())
                    .spawn()?;
            };

            return Ok(());
        } else if cursor.is_dir() {
            std::fs::write(&lastdir_path, &cursor.canonicalize()?.as_os_str().as_bytes())?;
            println!("\x00prompt\x1fFiles\n{}", generate_list(&cursor));
        } else {
            return Ok(());
        }
    } else {
        println!("\x00prompt\x1fFiles\n{}", generate_list(&cursor));
    }

    Ok(())
}
