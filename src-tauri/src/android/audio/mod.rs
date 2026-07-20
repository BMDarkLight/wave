//! Android ExoPlayer audio backend (JNI).
//!
//! Plays `content://` and `file://` URIs natively via Media3 ExoPlayer.
//! Queue / shuffle / repeat / media notifications stay in Rust + the
//! existing media-session plugin — this module is decode + output only.

#![cfg(target_os = "android")]

mod jni_bridge;

pub use jni_bridge::*;
