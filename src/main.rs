//! # EncryptedFS
//! An encrypted file system in Rust that mounts with FUSE. It can be used to create encrypted directories.
//!
//! # Usage
//!
//! To use the encrypted file system, you need to have FUSE installed on your system. You can install it by running the following command (or based on your distribution):
//!
//! ```bash
//! sudo apt-get update
//! sudo apt-get -y install fuse3
//! ```
//! A basic example of how to use the encrypted file system is shown below:
//!
//! ```
//! encrypted_fs --mount-point MOUNT_POINT --data-dir DATA_DIR
//! ```
//! Where `MOUNT_POINT` is the directory where the encrypted file system will be mounted and `DATA_DIR` is the directory where the encrypted data will be stored.\
//! It will prompt you to enter a password to encrypt/decrypt the data.
//!
//! ## Change Password
//! The encryption key is stored in a file and encrypted with a key derived from the password.
//! This offers the possibility to change the password without needing to decrypt and re-encrypt the whole data.
//! This is done by decrypting the key with the old password and re-encrypting it with the new password.
//!
//! To change the password, you can run the following command:
//! ```
//! encrypted_fs --change-password --data-dir DATA_DIR
//! ```
//! Where `DATA_DIR` is the directory where the encrypted data is stored.\
//! It will prompt you to enter the old password and then the new password.
//!
//! ## Encryption info
//! You can specify the encryption algorithm and derive key hash rounds adding these arguments to the command line:
//!
//! ```
//! --cipher CIPHER --derive-key-hash-rounds ROUNDS
//! ```
//! Where `CIPHER` is the encryption algorithm and `ROUNDS` is the number of rounds to derive the key hash.\
//! You can check the available ciphers with `encrypted_fs --help`.
//!
//! Default values are `ChaCha20` and `600_000` respectively.

use std::{env, io, panic, process};
use std::ffi::OsStr;
use std::io::Write;
use std::str::FromStr;

use clap::{Arg, ArgAction, ArgMatches, Command, crate_version};
use ctrlc::set_handler;
use fuse3::MountOptions;
use fuse3::raw::prelude::*;
use rpassword::read_password;
use strum::IntoEnumIterator;
use tokio::task;
use tracing::Level;

use encrypted_fs::encrypted_fs::{EncryptedFs, Cipher};
use encrypted_fs::encrypted_fs_fuse3::EncryptedFsFuse3;

#[tokio::main]
async fn main() {
    log_init();
    env_logger::init();

    let result = task::spawn_blocking(|| {
        panic::catch_unwind(|| {
            async_main()
        })
    }).await;

    // match result {
    //     Ok(Ok(_)) => println!("There was no panic"),
    //     Ok(Err(_)) | Err(_) => println!("A panic occurred"),
    // }
}

fn async_main() {
    let handle = tokio::runtime::Handle::current();
    handle.block_on(async {
        let matches = Command::new("EncryptedFS")
            .version(crate_version!())
            .author("Radu Marias")
            .arg(
                Arg::new("mount-point")
                    .long("mount-point")
                    .value_name("MOUNT_POINT")
                    .help("Act as a client, and mount FUSE at given path"),
            )
            .arg(
                Arg::new("data-dir")
                    .long("data-dir")
                    .required(true)
                    .value_name("data-dir")
                    .help("Where to store the encrypted data"),
            )
            .arg(
                Arg::new("cipher")
                    .long("cipher")
                    .value_name("cipher")
                    .default_value("ChaCha20")
                    .help(format!("Encryption type, possible values: {}",
                                  Cipher::iter().fold(String::new(), |mut acc, x| {
                                      acc.push_str(format!("{}{}{:?}", acc, if acc.len() != 0 { ", " } else { "" }, x).as_str());
                                      acc
                                  }).as_str()),
                    )
            )
            .arg(
                Arg::new("derive-key-hash-rounds")
                    .long("derive-key-hash-rounds")
                    .value_name("derive-key-hash-rounds")
                    .default_value("600000")
                    .help("How many times to hash the password to derive the key"),
            )
            .arg(
                Arg::new("auto_unmount")
                    .long("auto_unmount")
                    .action(ArgAction::SetTrue)
                    .help("Automatically unmount on process exit"),
            )
            .arg(
                Arg::new("allow-root")
                    .long("allow-root")
                    .action(ArgAction::SetTrue)
                    .help("Allow root user to access filesystem"),
            )
            .arg(
                Arg::new("allow-other")
                    .long("allow-other")
                    .action(ArgAction::SetTrue)
                    .help("Allow other user to access filesystem"),
            )
            .arg(
                Arg::new("direct-io")
                    .long("direct-io")
                    .action(ArgAction::SetTrue)
                    .requires("mount-point")
                    .help("Mount FUSE with direct IO"),
            )
            .arg(
                Arg::new("suid")
                    .long("suid")
                    .action(ArgAction::SetTrue)
                    .help("Enable setuid support when run as root"),
            )
            .arg(
                Arg::new("change-password")
                    .long("change-password")
                    .action(ArgAction::SetTrue)
                    .help("Change password for the encrypted data. Old password and new password with be read from stdin"),
            )
            .get_matches();

        let data_dir: String = matches
            .get_one::<String>("data-dir")
            .unwrap()
            .to_string();

        let cipher: String = matches
            .get_one::<String>("cipher")
            .unwrap()
            .to_string();
        let cipher = Cipher::from_str(cipher.as_str());
        if cipher.is_err() {
            println!("Invalid encryption type");
            return;
        }
        let cipher = cipher.unwrap();

        let derive_key_hash_rounds: String = matches
            .get_one::<String>("derive-key-hash-rounds")
            .unwrap()
            .to_string();
        let derive_key_hash_rounds = u32::from_str(derive_key_hash_rounds.as_str());
        if derive_key_hash_rounds.is_err() {
            println!("Invalid derive-key-hash-rounds");
            return;
        }
        let derive_key_hash_rounds = derive_key_hash_rounds.unwrap();

        if matches.get_flag("change-password") {
            // change password

            // read password from stdin
            print!("Enter old password: ");
            io::stdout().flush().unwrap();
            let password = read_password().unwrap();

            print!("Enter new password: ");
            io::stdout().flush().unwrap();
            let new_password = read_password().unwrap();
            EncryptedFs::change_password(&data_dir, &password, &new_password, &cipher, derive_key_hash_rounds).unwrap();
            println!("Password changed successfully");

            return;
        } else {
            //normal run

            if !matches.contains_id("mount-point") {
                println!("--mount-point <MOUNT_POINT> is required");
                return;
            }
            let mountpoint: String = matches.get_one::<String>("mount-point")
                .unwrap()
                .to_string();

            // when running from IDE we can't read from stdin with rpassword, get it from env var
            let mut password = env::var("ENCRYPTED_FS_PASSWORD").unwrap_or_else(|_| "".to_string());
            if password.is_empty() {
                // read password from stdin
                print!("Enter password: ");
                io::stdout().flush().unwrap();
                password = read_password().unwrap();
            }

            // unomunt(mountpoint.as_str());

            // unmount on process kill
            let mountpoint_kill = mountpoint.clone();
            set_handler(move || {
                unomunt(mountpoint_kill.as_str());
                process::exit(0);
            }).unwrap();

            run_fuse(matches, mountpoint, &data_dir, &password, cipher, derive_key_hash_rounds).await;
        }
    });
}

async fn run_fuse(matches: ArgMatches, mountpoint: String, data_dir: &str, password: &str, cipher: Cipher, derive_key_hash_rounds: u32) {
    let uid = unsafe { libc::getuid() };
    let gid = unsafe { libc::getgid() };

    let mut mount_options = MountOptions::default();
    mount_options.uid(uid).gid(gid).read_only(false);
    mount_options.allow_root(matches.get_flag("allow-root"));
    mount_options.allow_other(matches.get_flag("allow-other"));

    let mount_path = OsStr::new(mountpoint.as_str());
    Session::new(mount_options)
        .mount_with_unprivileged(EncryptedFsFuse3::new(&data_dir, &password, cipher, derive_key_hash_rounds,
                                                       matches.get_flag("direct-io"), matches.get_flag("suid")).unwrap(), mount_path)
        .await
        .unwrap()
        .await
        .unwrap();
}

fn unomunt(mountpoint: &str) {
    let output = process::Command::new("umount")
        .arg(mountpoint)
        .output()
        .expect("Failed to execute command");

    if output.status.success() {
        let result = String::from_utf8(output.stdout).unwrap();
        println!("{}", result);
    } else {
        let err = String::from_utf8(output.stderr).unwrap();
        println!("Error: {}", err);
    }
}

fn log_init() {
    let subscriber = tracing_subscriber::fmt()
        .with_max_level(Level::DEBUG)
        .finish();
    tracing::subscriber::set_global_default(subscriber).unwrap();
}
