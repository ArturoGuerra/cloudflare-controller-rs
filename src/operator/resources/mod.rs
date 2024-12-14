use thiserror::Error;

#[derive(Error, Debug)]
pub enum Error {}

/*
* Exported functions:
* Create
* Update
* Delete
*/

pub trait Resource<T> {
    fn create();
    fn upgrade();
    fn delete();
}

pub mod configmap;
pub mod deployment;
pub mod secret;
