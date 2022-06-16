extern crate time;

use std::env;
use std::ffi::OsStr;
use libc::{ENOENT, getuid, getgid};
use fuse::{FileType, FileAttr, Filesystem, Request, ReplyData, ReplyEntry, ReplyAttr, ReplyDirectory};
use time::Timespec;
use time::get_time;

const TTL: Timespec = Timespec { sec: 1, nsec: 0 };		         

static mut DIRECTIONS_ATTRIBUTES: Vec<(String, FileAttr)> = Vec::new();
static mut FILE_CONTENTS: Vec<&str> = Vec::new();
static mut FILE_ATTRIBUTES: Vec<(String, FileAttr)> = Vec::new();

struct HelloFS;

impl Filesystem for HelloFS {
    fn lookup(&mut self, _req: &Request, parent: u64, name: &OsStr, reply: ReplyEntry) {
        if parent == 1 && name.to_str() == Some("Hello.txt") {
            reply.entry(&TTL, unsafe{&FILE_ATTRIBUTES.get(0).unwrap().1}, 0);
        } else {
            reply.error(ENOENT);
        }
    }

    fn getattr(&mut self, _req: &Request, ino: u64, reply: ReplyAttr) {
        println!("operation getattr");
        match ino {
            1 => reply.attr(&TTL, unsafe{&DIRECTIONS_ATTRIBUTES.get(0).unwrap().1}),
            2 => reply.attr(&TTL, unsafe{&FILE_ATTRIBUTES.get(0).unwrap().1}),
            3 => reply.attr(&TTL, unsafe{&DIRECTIONS_ATTRIBUTES.get(1).unwrap().1}),
            _ => reply.error(ENOENT),
        }
    }

    fn read(&mut self, _req: &Request, ino: u64, _fh: u64, offset: i64, _size: u32, reply: ReplyData) {
        println!("operation read");
        if ino == 2 {
            reply.data(unsafe{&FILE_CONTENTS.get(0).unwrap().as_bytes()[offset as usize..]});
        } else {
            reply.error(ENOENT);
        }
    }

    fn readdir(&mut self, _req: &Request, ino: u64, _fh: u64, offset: i64, mut reply: ReplyDirectory) {
        println!("operation readdir");
        if ino != 1 {
            reply.error(ENOENT);
            return;
        }

        let mut entries = vec![
            (1, FileType::Directory, "."),
            (1, FileType::Directory, "..")
        ];

        unsafe {
            for directory in DIRECTIONS_ATTRIBUTES.iter() {
                if &directory.0 != "/" {
                    entries.push((2, directory.1.kind, &directory.0));
                }
            }

            for file in FILE_ATTRIBUTES.iter() {
                entries.push((2, file.1.kind, &file.0));
            }
        }

        for (i, entry) in entries.into_iter().enumerate().skip(offset as usize) {
            // i + 1 means the index of the next entry
            reply.add(entry.0, (i + 1) as i64, entry.1, entry.2);
        }
        reply.ok();
    }

    fn mkdir(&mut self, _req: &Request, _parent: u64, _name: &OsStr, _mode: u32, reply: ReplyEntry) {
        let create_time = get_time();
        let new_dir: FileAttr = FileAttr {
            ino: 3,
            size: 512,
            blocks: 0,
            atime: create_time,                                  // 1970-01-01 00:00:00
            mtime: create_time,
            ctime: create_time,
            crtime: create_time,
            kind: FileType::Directory,
            perm: 0o644,
            nlink: 2,
            uid: unsafe{getuid()},
            gid: unsafe{getgid()},
            rdev: 0,
            flags: 0,
        };

        let name_dir = _name.to_str().unwrap().to_owned();
        unsafe{DIRECTIONS_ATTRIBUTES.push((name_dir, new_dir));}

        reply.entry(&TTL, &new_dir, 0);
    }
}

fn main() {
    let create_time = get_time();

    let hello_dir: FileAttr = FileAttr {
        ino: 1,
        size: 0,
        blocks: 0,
        atime: create_time,                                  // 1970-01-01 00:00:00
        mtime: create_time,
        ctime: create_time,
        crtime: create_time,
        kind: FileType::Directory,
        perm: 0o755,
        nlink: 2,
        uid: unsafe{getuid()},
        gid: unsafe{getgid()},
        rdev: 0,
        flags: 0,
    };

    unsafe{DIRECTIONS_ATTRIBUTES.push((String::from("/") ,hello_dir));}

    let hello_content: &str = "Hello World!\n";

    unsafe{FILE_CONTENTS.push(hello_content);}

    let hello_attr: FileAttr = FileAttr {
        ino: 2,
        size: 13,
        blocks: 1,
        atime: create_time,                                  // 1970-01-01 00:00:00
        mtime: create_time,
        ctime: create_time,
        crtime: create_time,
        kind: FileType::RegularFile,
        perm: 0o644,
        nlink: 1,
        uid: unsafe{getuid()},
        gid: unsafe{getgid()},
        rdev: 0,
        flags: 0,
    };

    unsafe{FILE_ATTRIBUTES.push((String::from("Hello.txt"), hello_attr));}
    
    env_logger::init();
    let mountpoint = env::args_os().nth(1).unwrap();
    fuse::mount(HelloFS, &mountpoint, &Vec::new()).unwrap();
    //cargo run path (it must be empty)
    //cargo run /home/kzumbado/Documents/proyecto2-sistemas-operativos/qrfs/tmp
}