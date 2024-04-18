use std::thread;
use tokio::process::Command;

#[tokio::main]
async fn main() {
    let mut child = Command::new("/home/gnome/dev/RustroverProjects/encrypted_fs/target/debug/encrypted_fs")
        .env("ENCRYPTED_FS_PASSWORD", "pass-42")
        .arg("--mount-point")
        .arg("/home/gnome/encrypted_fs")
        .arg("--data-dir")
        .arg("/home/gnome/encrypted_fs_data")
        .arg("--umount-on-start")
        .spawn()
        .expect("Failed to start process");

    thread::spawn(|| {
        thread::sleep(std::time::Duration::from_secs(10));
    }).join().expect("Thread panicked");
    child.kill().await.unwrap();
    println!("killed");

    thread::spawn(|| {
        thread::sleep(std::time::Duration::from_secs(60));
    }).join().expect("Thread panicked");
}