use thiserror::Error;

#[derive(Error, Debug)]
pub enum Error {}

// TODO: Create an interface for this that is bound to the CRD resource.

/*
* Exported functions:
* Create
* Update
* Delete
*/

pub mod configmap;
pub mod deployment;
pub mod secret;
