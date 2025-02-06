use kube::CustomResourceExt;
use std::{env, fs::File, io::Write};

pub mod cloudflare;
pub mod controllers;
pub mod crd;

fn main() {
    let current_dir = env::current_dir().unwrap();

    let credentials_crd =
        serde_json::to_string_pretty(&crd::credentials::Credentials::crd()).unwrap();

    let mut file =
        File::create(current_dir.join("credentials_crd.yaml")).expect("unable to create file");

    file.write_all(credentials_crd.into_bytes().as_ref())
        .unwrap();

    let tunnels_crd = serde_json::to_string_pretty(&crd::tunnel::Tunnel::crd()).unwrap();

    let mut file =
        File::create(current_dir.join("tunnel_crd.yaml")).expect("unable to create file");

    file.write_all(tunnels_crd.into_bytes().as_ref()).unwrap();
}
