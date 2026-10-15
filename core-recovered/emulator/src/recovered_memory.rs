// SPDX-License-Identifier: MPL-2.0
//! Bounded backing-memory access for recovered host services. Never reads MMIO.
pub trait GuestMemory {
    fn bytes(&self, address: u32, count: usize) -> Option<&[u8]>;
    fn ram_mut(&mut self, address: u32, count: usize) -> Option<&mut [u8]>;
    fn u8(&self, address: u32) -> Option<u8> {
        self.bytes(address, 1).map(|b| b[0])
    }
    fn u16(&self, address: u32) -> Option<u16> {
        self.bytes(address, 2)
            .map(|b| u16::from_be_bytes(b.try_into().unwrap()))
    }
    fn u32(&self, address: u32) -> Option<u32> {
        self.bytes(address, 4)
            .map(|b| u32::from_be_bytes(b.try_into().unwrap()))
    }
}
#[cfg(test)]
pub struct TestMemory(pub Vec<u8>);
#[cfg(test)]
impl GuestMemory for TestMemory {
    fn bytes(&self, a: u32, n: usize) -> Option<&[u8]> {
        self.0.get(a as usize..(a as usize).checked_add(n)?)
    }
    fn ram_mut(&mut self, a: u32, n: usize) -> Option<&mut [u8]> {
        self.0.get_mut(a as usize..(a as usize).checked_add(n)?)
    }
}
