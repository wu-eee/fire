use caps::{all, clear, set, CapSet, Capability};
use log::{debug, warn};
use oci::{LinuxCapabilities, LinuxCapabilityType};
use std::collections::HashSet;

use crate::errors::*;

fn to_cap(cap: LinuxCapabilityType) -> Capability {
    unsafe { ::std::mem::transmute(cap) }
}

fn to_set(caps: &[LinuxCapabilityType]) -> HashSet<Capability> {
    let mut capabilities = HashSet::new();
    for c in caps {
        capabilities.insert(to_cap(*c));
    }
    capabilities
}

pub fn reset_effective() -> Result<()> {
    clear(None, CapSet::Effective)?;
    set(None, CapSet::Effective, &all())?;
    Ok(())
}

pub fn drop_privileges(cs: &LinuxCapabilities) -> Result<()> {
    let all_caps = all();
    debug!("dropping bounding capabilities to {:?}", cs.bounding);
    // drop excluded caps from the bounding set
    for c in all_caps.difference(&to_set(&cs.bounding)) {
        caps::drop(None, CapSet::Bounding, *c)?;
    }
    // set other sets for current process
    set(None, CapSet::Effective, &to_set(&cs.effective))?;
    set(None, CapSet::Permitted, &to_set(&cs.permitted))?;
    set(None, CapSet::Inheritable, &to_set(&cs.inheritable))?;
    if let Err(e) = set(None, CapSet::Ambient, &to_set(&cs.ambient)) {
        warn!("failed to set ambient capabilities: {}", e);
    }
    Ok(())
}
