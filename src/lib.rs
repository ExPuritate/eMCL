#![feature(ascii_char)]
#![feature(ascii_char_variants)]
#![feature(sync_nonpoison)]
#![feature(nonpoison_rwlock)]
#![feature(once_cell_try)]
#![feature(box_into_inner)]
#![feature(alloc_slice_into_array)]
#![feature(transmute_prefix)]
#![feature(file_buffered)]
#![feature(mapped_lock_guards)]

pub mod configuration;
pub mod environment;
pub mod error;
pub mod java;
pub mod launch;
pub mod manifest;
pub mod sha1_serde;
