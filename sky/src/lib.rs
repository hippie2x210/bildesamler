//! Skytjenester for CloudJumper. Bare lesetilgang (Gmail, OneDrive og Dropbox): ingenting her kan slette eller endre noe i skyen.

pub mod dropbox;
pub mod dropbox_hash;
pub mod google;
pub mod oauth;
pub mod onedrive;
pub mod quickxor;
