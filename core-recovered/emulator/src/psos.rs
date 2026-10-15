// SPDX-License-Identifier: MPL-2.0
//! OEM host pSOS kernel bridge, ported from recovered psos.cpp contracts.
//! Recognizes the exact blank module header and check_page callback. The guest
//! owns ROOT, task entry points and messages; no menus or ECU data are supplied.
use crate::bus::{Tech2Bus, RAM_BASE, RAM_SIZE};
use m68k::{AddressBus, CpuCore};
use std::collections::{BTreeMap, VecDeque};
use tech2_emu::recovered_memory::GuestMemory;
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum State {
    #[default]
    Created,
    Ready,
    Running,
    Blocked,
    Dead,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum Wait {
    #[default]
    None,
    Events,
    Semaphore,
    Queue,
    Sleep,
}
#[derive(Clone, Debug, Default)]
struct Context {
    dar: [u32; 16],
    sp: [u32; 8],
    pc: u32,
    sr: u16,
    vbr: u32,
}
impl Context {
    fn capture(cpu: &CpuCore) -> Self {
        let mut sp = cpu.sp;
        sp[(cpu.s_flag | ((cpu.s_flag >> 1) & cpu.m_flag)) as usize] = cpu.sp();
        Self {
            dar: cpu.dar,
            sp,
            pc: cpu.pc,
            sr: cpu.get_sr(),
            vbr: cpu.vbr,
        }
    }
    fn restore(&self, cpu: &mut CpuCore) {
        cpu.set_sr_noint_nosp(self.sr);
        cpu.sp = self.sp;
        cpu.dar = self.dar;
        cpu.pc = self.pc;
        cpu.vbr = self.vbr;
        cpu.stopped = 0;
        cpu.prefetch_count = 0;
    }
}
#[derive(Clone, Debug, Default)]
struct Task {
    id: u32,
    name: u32,
    priority: u32,
    mode: u32,
    events: u32,
    mask: u32,
    wait_flags: u32,
    wait_object: u32,
    state: State,
    wait: Wait,
    until: u64,
    suspended: bool,
    cpu: Context,
    regs: [u32; 17],
    stack: u32,
    stack_size: u32,
    super_stack: u32,
    super_size: u32,
    entry: u32,
}
#[derive(Clone, Debug, Default)]
struct Region {
    name: u32,
    unit: u32,
    free: BTreeMap<u32, u32>,
    used: BTreeMap<u32, u32>,
}
#[derive(Clone, Debug)]
struct Semaphore {
    name: u32,
    count: u32,
}
#[derive(Clone, Debug)]
struct Queue {
    name: u32,
    flags: u32,
    capacity: u32,
    messages: VecDeque<[u32; 4]>,
}
#[derive(Clone, Debug)]
struct Timer {
    task: u32,
    events: u32,
    due: u64,
    period: u64,
}
#[derive(Clone, Debug)]
pub struct Kernel {
    pub active: bool,
    pub kernel_entry: u32,
    pub error: Option<String>,
    pub calls: u64,
    scanned: bool,
    kernel_header: u32,
    kernel_config: u32,
    current: u32,
    next_task: u32,
    next_object: u32,
    next_queue: u32,
    next_timer: u32,
    return_entry: u32,
    page_table: u32,
    page_task_word: u32,
    ticks: u64,
    last_host_tick: u64,
    switches: u64,
    date: u32,
    time: u32,
    tasks: BTreeMap<u32, Task>,
    regions: BTreeMap<u32, Region>,
    semaphores: BTreeMap<u32, Semaphore>,
    queues: BTreeMap<u32, Queue>,
    timers: BTreeMap<u32, Timer>,
    counts: BTreeMap<u32, u64>,
}
impl Default for Kernel {
    fn default() -> Self {
        Self {
            active: false,
            kernel_entry: 0,
            error: None,
            calls: 0,
            scanned: false,
            kernel_header: 0,
            kernel_config: 0,
            current: 0,
            next_task: 1,
            next_object: 1,
            next_queue: 1,
            next_timer: 1,
            return_entry: 0,
            page_table: 0,
            page_task_word: 0,
            ticks: 0,
            last_host_tick: 0,
            switches: 0,
            date: 0,
            time: 0,
            tasks: BTreeMap::new(),
            regions: BTreeMap::new(),
            semaphores: BTreeMap::new(),
            queues: BTreeMap::new(),
            timers: BTreeMap::new(),
            counts: BTreeMap::new(),
        }
    }
}
fn ram_range(address: u32, bytes: u32) -> bool {
    address >= RAM_BASE
        && u64::from(address) + u64::from(bytes) <= u64::from(RAM_BASE) + u64::from(RAM_SIZE)
}
impl Kernel {
    fn scan(&mut self, bus: &Tech2Bus) {
        self.scanned = true;
        for offset in (0x8000..bus.flash.len().saturating_sub(32)).step_by(2) {
            let b = &bus.flash;
            if b[offset + 4..offset + 24]
                != [
                    0x06, 0xff, 0xa2, 0x84, 0, 0, 0x39, 0x8c, 0x4e, 0x4b, 0x4e, 0x75, 0x4e, 0x4c,
                    0x4e, 0x75, 0x4e, 0x4d, 0, 1,
                ]
                || b[offset + 24..offset + 32] != [0; 8]
            {
                continue;
            }
            let Some(body) = b.get(offset + 32..offset + 0x398c) else {
                continue;
            };
            // Complete support kernels retain their own native implementation.
            if !body.iter().all(|byte| *byte == 0) {
                continue;
            }
            self.kernel_header = offset as u32;
            self.kernel_entry = offset as u32 + 32;
            break;
        }
    }
    fn allocate(&mut self, region: u32, bytes: u32) -> u32 {
        let Some(r) = self.regions.get_mut(&region) else {
            return 0;
        };
        if bytes == 0 || r.unit == 0 {
            return 0;
        }
        let n = (u64::from(bytes).div_ceil(u64::from(r.unit)) * u64::from(r.unit)) as u64;
        if n > 0x1000000 {
            return 0;
        }
        let n = n as u32;
        let Some((base, size)) = r
            .free
            .iter()
            .find(|(_, size)| **size >= n)
            .map(|(b, s)| (*b, *s))
        else {
            return 0;
        };
        r.free.remove(&base);
        if size > n {
            r.free.insert(base + n, size - n);
        }
        r.used.insert(base, n);
        base
    }
    fn create(
        &mut self,
        name: u32,
        priority: u32,
        user: u32,
        super_size: u32,
        bus: &Tech2Bus,
        cpu: &CpuCore,
    ) -> (u32, u32) {
        if priority > 255 {
            return (0x11, 0);
        }
        if super_size != 0 && super_size < 21 {
            return (0x10, 0);
        }
        if self.next_task > 255
            || self.tasks.len() > bus.u32(self.kernel_config + 0x10).unwrap_or(0) as usize + 2
        {
            return (0x0e, 0);
        }
        let id = self.next_task << 16;
        self.next_task += 1;
        let stack = if user != 0 { self.allocate(0, user) } else { 0 };
        let super_stack = if super_size != 0 {
            self.allocate(0, super_size)
        } else {
            0
        };
        if (user != 0 && stack == 0) || (super_size != 0 && super_stack == 0) {
            return (0x0f, 0);
        }
        self.tasks.insert(
            id,
            Task {
                id,
                name,
                priority,
                stack,
                stack_size: user,
                super_stack,
                super_size,
                cpu: Context {
                    vbr: cpu.vbr,
                    ..Default::default()
                },
                ..Default::default()
            },
        );
        (0, id)
    }
    fn task_id(&self, id: u32) -> u32 {
        if id == 0 {
            self.current
        } else {
            id
        }
    }
    fn start(&mut self, id: u32, mode: u32, pc: u32, args: [u32; 4], bus: &mut Tech2Bus) -> u32 {
        let id = self.task_id(id);
        let Some(t) = self.tasks.get_mut(&id) else {
            return 5;
        };
        if pc == 0 {
            return 0x18;
        }
        let mut top = if mode & 0x2000 != 0 {
            t.stack + t.stack_size
        } else {
            t.super_stack + t.super_size
        };
        if top == 0 {
            top = t.stack + t.stack_size;
        }
        top &= !3;
        if top < 20 || !ram_range(top - 20, 20) {
            return 0x0f;
        }
        for arg in args.iter().rev() {
            top -= 4;
            bus.write_long(top, *arg);
        }
        top -= 4;
        bus.write_long(top, self.return_entry);
        t.entry = pc;
        t.mode = mode;
        t.cpu.sr = (mode & 0xa71f) as u16;
        t.cpu.pc = pc;
        t.cpu.dar[15] = top;
        t.cpu.sp[4] = if mode & 0x2000 != 0 {
            top
        } else {
            t.stack + t.stack_size
        };
        t.cpu.sp[0] = if mode & 0x2000 != 0 {
            t.super_stack + t.super_size
        } else {
            top
        };
        t.state = State::Ready;
        0
    }
    fn select_page(&self, bus: &mut Tech2Bus) {
        if self.current == 0 || self.page_table == 0 {
            return;
        }
        let index = self.current >> 16;
        if let Some(bank) = bus.u8(self.page_table + index) {
            bus.write_byte(crate::bus::PCMCIA_BANK_REG, bank);
            bus.write_word(self.page_task_word, index as u16);
        }
    }
    fn schedule(&mut self, cpu: &mut CpuCore, bus: &mut Tech2Bus, force: bool) {
        let best = self
            .tasks
            .values()
            .filter(|t| !t.suspended && matches!(t.state, State::Ready | State::Running))
            .fold(None::<(u32, u32)>, |best, t| match best {
                Some((_, priority)) if priority >= t.priority => best,
                _ => Some((t.id, t.priority)),
            });
        let Some((id, priority)) = best else {
            return;
        };
        if let Some(now) = self.tasks.get(&self.current) {
            if matches!(now.state, State::Running | State::Ready)
                && !now.suspended
                && !force
                && (cpu.get_sr() & 0x700 != 0 || now.mode & 1 != 0 || priority <= now.priority)
            {
                return;
            }
        }
        if id == self.current {
            return;
        }
        if let Some(now) = self.tasks.get_mut(&self.current) {
            if now.state != State::Dead {
                now.cpu = Context::capture(cpu);
                if now.state == State::Running {
                    now.state = State::Ready;
                }
            }
        }
        self.current = id;
        let next = self.tasks.get_mut(&id).unwrap();
        next.state = State::Running;
        next.cpu.restore(cpu);
        self.select_page(bus);
        self.switches += 1;
        if self.switches <= 16 {
            bus.trace_event(
                "psos_task_switch",
                &format!(
                    "task={id:#x} pc={:#x} priority={priority} backend=recovered-host-kernel",
                    cpu.pc
                ),
            );
        }
    }
    fn block(&mut self, wait: Wait, object: u32, timeout: u32) {
        if let Some(t) = self.tasks.get_mut(&self.current) {
            t.state = State::Blocked;
            t.wait = wait;
            t.wait_object = object;
            t.until = if timeout == 0 {
                u64::MAX
            } else {
                self.ticks.saturating_add(u64::from(timeout))
            };
        }
    }
    fn send_event(&mut self, id: u32, bits: u32) -> u32 {
        let id = self.task_id(id);
        let Some(t) = self.tasks.get_mut(&id) else {
            return 5;
        };
        t.events |= bits;
        if t.wait == Wait::Events {
            let matched = t.events & t.mask;
            let ready = if t.wait_flags & 2 != 0 {
                matched != 0
            } else {
                matched == t.mask
            };
            if ready {
                t.events &= !matched;
                t.cpu.dar[0] = 0;
                t.cpu.dar[4] = matched;
                t.wait = Wait::None;
                t.state = State::Ready;
                t.until = 0;
            }
        }
        0
    }
    fn tick(&mut self) {
        self.ticks = self.ticks.saturating_add(1);
        for t in self.tasks.values_mut() {
            if t.state == State::Blocked && t.until != u64::MAX && t.until <= self.ticks {
                t.state = State::Ready;
                t.cpu.dar[0] = if t.wait == Wait::Sleep { 0 } else { 1 };
                t.wait = Wait::None;
                t.until = 0;
            }
        }
        let due: Vec<_> = self
            .timers
            .iter()
            .filter(|(_, t)| t.due <= self.ticks)
            .map(|(id, t)| (*id, t.clone()))
            .collect();
        for (id, timer) in due {
            self.send_event(timer.task, timer.events);
            if timer.period == 0 {
                self.timers.remove(&id);
            } else {
                self.timers.get_mut(&id).unwrap().due = timer.due.saturating_add(timer.period);
            }
        }
    }
    fn initialize(&mut self, cpu: &mut CpuCore, bus: &mut Tech2Bus) -> Result<(), String> {
        let pointer = bus
            .u32(self.kernel_header)
            .ok_or("pSOS module header outside mapped memory")?;
        let global = bus
            .u32(pointer)
            .ok_or("pSOS global pointer outside mapped memory")?;
        let config = bus
            .u32(
                global
                    .checked_add(8)
                    .ok_or("pSOS config pointer overflow")?,
            )
            .ok_or("pSOS config pointer unavailable")?;
        if !ram_range(config, 0x80) {
            return Err("pSOS kernel config outside RAM".into());
        }
        self.kernel_config = config;
        let check = bus.u32(config + 0x54).unwrap();
        let callback = bus
            .bytes(check, 24)
            .ok_or("pSOS check_page callback outside memory")?;
        if callback[..6] != [0x48, 0x41, 0x13, 0xf0, 0x11, 0xb0]
            || callback[10..16] != [0, 0x60, 6, 0, 0x33, 0xc1]
            || callback[20..24] != [0x48, 0x41, 0x4e, 0x75]
        {
            return Err("Unsupported pSOS check_page callback".into());
        }
        self.page_table = bus.u32(check + 6).unwrap();
        self.page_task_word = bus.u32(check + 16).unwrap();
        if !ram_range(self.page_table, 256) || !ram_range(self.page_task_word, 2) {
            return Err("pSOS check_page operands outside RAM".into());
        }
        let base = bus.u32(config + 4).unwrap();
        let size = bus.u32(config + 8).unwrap();
        let unit = bus.u32(config + 12).unwrap().max(1);
        if !ram_range(base, size) || size == 0 {
            return Err("pSOS RN0 outside RAM".into());
        }
        self.regions.insert(
            0,
            Region {
                name: 0x524e3020,
                unit,
                free: BTreeMap::from([(base, size)]),
                used: BTreeMap::new(),
            },
        );
        self.return_entry = self.allocate(0, 32);
        let idle = self.allocate(0, 8);
        if self.return_entry == 0 || idle == 0 {
            return Err("Cannot allocate pSOS glue".into());
        }
        for (n, word) in [0x7200, 0x7005, 0x4e4b, 0x4e72, 0x2000, 0x60fa]
            .iter()
            .enumerate()
        {
            bus.write_word(self.return_entry + n as u32 * 2, *word);
        }
        for (n, word) in [0x4e72, 0x2000, 0x60fa].iter().enumerate() {
            bus.write_word(idle + n as u32 * 2, *word);
        }
        let user = bus.u32(config + 0x40).unwrap();
        let supervisor = bus.u32(config + 0x44).unwrap();
        let (root_code, root) = self.create(0x524f4f54, 255, user, supervisor, bus, cpu);
        let (idle_code, idle_task) = self.create(0x49444c45, 0, user, 0, bus, cpu);
        if root_code != 0 || idle_code != 0 {
            return Err("Cannot create pSOS ROOT/IDLE".into());
        }
        let root_pc = bus.u32(config + 0x3c).unwrap();
        let root_mode = bus.u32(config + 0x48).unwrap();
        if self.start(root, root_mode, root_pc, [0; 4], bus) != 0
            || self.start(idle_task, 0x2000, idle, [0; 4], bus) != 0
        {
            return Err("Cannot start pSOS ROOT/IDLE".into());
        }
        self.active = true;
        self.last_host_tick = bus.current_insns;
        bus.trace_event("psos_initialize",&format!("kernel={:#x} config={config:#x} root={root_pc:#x} rn0={base:#x}+{size:#x} backend=recovered-host-kernel",self.kernel_entry));
        self.schedule(cpu, bus, true);
        Ok(())
    }
    pub fn before_instruction(&mut self, cpu: &mut CpuCore, bus: &mut Tech2Bus) {
        if !self.scanned && (0x8000..0x40000).contains(&cpu.pc) {
            self.scan(bus);
        }
        if !self.active && self.kernel_entry != 0 && cpu.pc == self.kernel_entry {
            if let Err(error) = self.initialize(cpu, bus) {
                self.error = Some(error.clone());
                bus.failure_reason = Some(error);
            }
            return;
        }
        if self.active {
            if bus.current_insns.saturating_sub(self.last_host_tick) >= 100000 {
                self.last_host_tick = bus.current_insns;
                self.tick();
            }
            self.schedule(cpu, bus, false);
        }
    }
    pub fn rebase(&mut self, captured: u64, now: u64) {
        self.last_host_tick = now.saturating_sub(captured.saturating_sub(self.last_host_tick));
    }
    pub fn snapshot(&self) -> serde_json::Value {
        serde_json::json!({"active":self.active,"kernel_entry":self.kernel_entry,"current_task":self.current,"tasks":self.tasks.len(),"calls":self.calls,"switches":self.switches,"ticks":self.ticks,"counts":self.counts,"error":self.error})
    }
    pub fn trap(
        &mut self,
        number: u8,
        file_result: Option<u32>,
        cpu: &mut CpuCore,
        bus: &mut Tech2Bus,
    ) -> bool {
        if !self.active {
            return false;
        }
        if number == 13 {
            let sp = cpu.sp();
            let parsed = (|| {
                let sr = bus.u16(sp)?;
                let pc = bus.u32(sp.checked_add(2)?)?;
                let format = bus.u16(sp.checked_add(6)?)? >> 12;
                let len = match format {
                    0 => 8,
                    2 => 12,
                    12 => 24,
                    _ => return None,
                };
                bus.bytes(sp, len as usize)?;
                Some((sr, pc, len))
            })();
            if let Some((sr, pc, len)) = parsed {
                cpu.set_sp(sp + len);
                cpu.set_sr(sr);
                cpu.pc = pc;
                cpu.prefetch_count = 0;
                self.schedule(cpu, bus, false);
                self.select_page(bus);
            } else {
                let error = "Unsupported pSOS TRAP13 RTE frame".to_string();
                self.error = Some(error.clone());
                bus.failure_reason = Some(error);
            }
            return true;
        }
        if number != 11 {
            return false;
        }
        self.calls += 1;
        let r = cpu.dar;
        let op = r[0];
        *self.counts.entry(op).or_default() += 1;
        let id = self.task_id(r[1]);
        let mut code = 0;
        if self.counts[&op] <= 3 {
            bus.trace_event(
                "psos_service",
                &format!(
                    "op={op:#x} task={:#x} pc={:#x} d1={:#x} d2={:#x} d6={:#x}",
                    self.current, cpu.pc, r[1], r[2], r[6]
                ),
            );
        }
        if let Some(result) = file_result {
            code = result;
        } else {
            match op {
                1 => {
                    let (status, task) = self.create(r[1], r[2], r[3], r[4], bus, cpu);
                    code = status;
                    cpu.set_d(1, task);
                }
                2 => {
                    let task = if r[1] == 0 {
                        self.current
                    } else {
                        self.tasks
                            .values()
                            .find(|t| t.name == r[1] && t.state != State::Dead)
                            .map_or(0, |t| t.id)
                    };
                    code = if r[2] != 0 {
                        4
                    } else if task == 0 {
                        9
                    } else {
                        0
                    };
                    cpu.set_d(1, task);
                }
                3 => code = self.start(id, r[2], r[8], [r[3], r[4], r[5], r[6]], bus),
                4 => {
                    if let Some(t) = self.tasks.get(&id) {
                        code = self.start(id, t.mode, t.entry, [r[2], r[3], r[4], r[5]], bus);
                    } else {
                        code = 5;
                    }
                }
                5 => {
                    if let Some(t) = self.tasks.get_mut(&id) {
                        t.state = State::Dead;
                    } else {
                        code = 5;
                    }
                }
                6 | 7 => {
                    if let Some(t) = self.tasks.get_mut(&id) {
                        let suspend = op == 6;
                        if t.suspended == suspend {
                            code = if suspend { 0x14 } else { 0x15 };
                        } else {
                            t.suspended = suspend;
                        }
                    } else {
                        code = 5;
                    }
                }
                8 => {
                    if let Some(t) = self.tasks.get_mut(&id) {
                        if r[2] > 255 {
                            code = 0x16;
                        } else {
                            cpu.set_d(2, t.priority);
                            if r[2] != 0 {
                                t.priority = r[2];
                            }
                        }
                    } else {
                        code = 5;
                    }
                }
                9 => {
                    if let Some(t) = self.tasks.get_mut(&self.current) {
                        cpu.set_d(2, t.mode);
                        if r[1] & 0x2000 != 0 {
                            code = 99;
                        } else {
                            t.mode = (t.mode & !r[1]) | (r[1] & r[2]);
                            if r[1] & 0x700 != 0 {
                                cpu.set_sr((cpu.get_sr() & !0x700) | (t.mode as u16 & 0x700));
                            }
                        }
                    } else {
                        code = 5;
                    }
                }
                10 | 11 => {
                    if let Some(t) = self.tasks.get_mut(&id) {
                        if r[2] > 16 {
                            code = 0x17;
                        } else if op == 10 {
                            cpu.set_d(3, t.regs[r[2] as usize]);
                        } else {
                            t.regs[r[2] as usize] = r[3];
                        }
                    } else {
                        code = 5;
                    }
                }
                12 => {
                    cpu.set_d(0, self.current);
                    self.select_page(bus);
                    return true;
                }
                0xe => {
                    let base = r[9];
                    let size = r[2];
                    let unit = r[3];
                    if base & 3 != 0 || unit == 0 {
                        code = 0x1b;
                    } else if size < unit {
                        code = 0x1e;
                    } else if !ram_range(base, size) {
                        code = 0x1b;
                    } else {
                        let region = self.regions.len() as u32;
                        self.regions.insert(
                            region,
                            Region {
                                name: r[1],
                                unit,
                                free: BTreeMap::from([(base, size)]),
                                used: BTreeMap::new(),
                            },
                        );
                        cpu.set_d(1, region);
                        cpu.set_d(2, size);
                    }
                }
                0xf => {
                    let found = self
                        .regions
                        .iter()
                        .find(|(_, v)| v.name == r[1])
                        .map(|(id, _)| *id);
                    code = if found.is_some() { 0 } else { 9 };
                    cpu.set_d(1, found.unwrap_or(0));
                }
                0x10 => {
                    if r[1] == 0 || self.regions.remove(&r[1]).is_none() {
                        code = 5;
                    }
                }
                0x11 => {
                    let pointer = self.allocate(r[1], r[3]);
                    cpu.dar[8] = pointer;
                    if pointer == 0 {
                        code = 0x21;
                    }
                }
                0x12 => {
                    if let Some(region) = self.regions.get_mut(&r[1]) {
                        if let Some(size) = region.used.remove(&r[8]) {
                            region.free.insert(r[8], size);
                        } else {
                            code = 0x1f;
                        }
                    } else {
                        code = 5;
                    }
                }
                0x24 => {
                    if self.queues.len() > bus.u32(self.kernel_config + 0x14).unwrap_or(0) as usize
                        || self.queues.len() >= 1024
                    {
                        code = 0x33;
                    } else {
                        let queue = self.next_queue;
                        self.next_queue += 1;
                        self.queues.insert(
                            queue,
                            Queue {
                                name: r[1],
                                flags: r[5],
                                capacity: if r[5] & 4 != 0 { r[2] } else { 0 },
                                messages: VecDeque::new(),
                            },
                        );
                        cpu.set_d(1, queue);
                    }
                }
                0x25 => {
                    let queue = self
                        .queues
                        .iter()
                        .find(|(_, q)| q.name == r[1])
                        .map_or(0, |(id, _)| *id);
                    code = if r[2] != 0 {
                        4
                    } else if queue == 0 {
                        9
                    } else {
                        0
                    };
                    cpu.set_d(1, queue);
                }
                0x26 => {
                    if self.queues.remove(&r[1]).is_none() {
                        code = 5;
                    } else {
                        for task in self.tasks.values_mut() {
                            if task.wait == Wait::Queue && task.wait_object == r[1] {
                                task.cpu.dar[0] = 0x36;
                                task.state = State::Ready;
                                task.wait = Wait::None;
                            }
                        }
                    }
                }
                0x27 | 0x28 | 0x29 => {
                    if let Some(q) = self.queues.get_mut(&r[1]) {
                        let message = [r[2], r[3], r[4], r[5]];
                        let mut waiters: Vec<_> = self
                            .tasks
                            .values()
                            .filter(|t| t.wait == Wait::Queue && t.wait_object == r[1])
                            .map(|t| (t.id, t.priority))
                            .collect();
                        if q.flags & 2 != 0 {
                            waiters.sort_by_key(|(_, priority)| std::cmp::Reverse(*priority));
                        }
                        if !waiters.is_empty() {
                            let count = if op == 0x29 { waiters.len() } else { 1 };
                            for (id, _) in waiters.into_iter().take(count) {
                                let task = self.tasks.get_mut(&id).unwrap();
                                task.cpu.dar[0] = 0;
                                task.cpu.dar[2..6].copy_from_slice(&message);
                                task.wait = Wait::None;
                                task.state = State::Ready;
                                task.until = 0;
                            }
                        } else if op != 0x29 {
                            if (q.capacity != 0 && q.messages.len() >= q.capacity as usize)
                                || q.messages.len() >= 65536
                            {
                                code = 0x35;
                            } else if op == 0x28 {
                                q.messages.push_front(message);
                            } else {
                                q.messages.push_back(message);
                            }
                        }
                        if op == 0x29 {
                            cpu.set_d(2, 0);
                        }
                    } else {
                        code = 5;
                    }
                }
                0x2a => {
                    if let Some(q) = self.queues.get_mut(&r[1]) {
                        if let Some(message) = q.messages.pop_front() {
                            for (n, word) in message.iter().enumerate() {
                                cpu.set_d(n + 2, *word);
                            }
                        } else if r[5] & 1 != 0 {
                            code = 0x37;
                            for n in 2..6 {
                                cpu.set_d(n, 0);
                            }
                        } else {
                            for n in 2..6 {
                                cpu.set_d(n, 0);
                            }
                            self.block(Wait::Queue, r[1], r[6]);
                        }
                    } else {
                        code = 5;
                    }
                }
                0x2c => code = self.send_event(r[1], r[4]),
                0x2d => {
                    if let Some(t) = self.tasks.get_mut(&self.current) {
                        if r[4] == 0 {
                            cpu.set_d(4, t.events);
                        } else {
                            let bits = t.events & r[4];
                            let ready = if r[5] & 2 != 0 {
                                bits != 0
                            } else {
                                bits == r[4]
                            };
                            if ready {
                                t.events &= !bits;
                                cpu.set_d(4, bits);
                            } else if r[5] & 1 != 0 {
                                code = 0x3c;
                            } else {
                                t.mask = r[4];
                                t.wait_flags = r[5];
                                cpu.set_d(4, 0);
                                self.block(Wait::Events, 0, r[6]);
                            }
                        }
                    } else {
                        code = 5;
                    }
                }
                0x33 => {
                    if self.semaphores.len() >= 1024 {
                        code = 0x41;
                    } else {
                        let semaphore = self.next_object;
                        self.next_object += 1;
                        self.semaphores.insert(
                            semaphore,
                            Semaphore {
                                name: r[1],
                                count: r[2],
                            },
                        );
                        cpu.set_d(1, semaphore);
                    }
                }
                0x34 => {
                    let sem = self
                        .semaphores
                        .iter()
                        .find(|(_, s)| s.name == r[1])
                        .map_or(0, |(id, _)| *id);
                    code = if r[2] != 0 {
                        4
                    } else if sem == 0 {
                        9
                    } else {
                        0
                    };
                    cpu.set_d(1, sem);
                }
                0x35 => {
                    if self.semaphores.remove(&r[1]).is_none() {
                        code = 5;
                    }
                }
                0x36 => {
                    if let Some(s) = self.semaphores.get_mut(&r[1]) {
                        if s.count != 0 {
                            s.count -= 1;
                        } else if r[5] & 1 != 0 {
                            code = 0x42;
                        } else {
                            self.block(Wait::Semaphore, r[1], r[6]);
                        }
                    } else {
                        code = 5;
                    }
                }
                0x37 => {
                    if let Some(s) = self.semaphores.get_mut(&r[1]) {
                        let waiter = self
                            .tasks
                            .values()
                            .filter(|t| t.wait == Wait::Semaphore && t.wait_object == r[1])
                            .fold(None::<(u32, u32)>, |best, t| match best {
                                Some((_, priority)) if priority >= t.priority => best,
                                _ => Some((t.id, t.priority)),
                            });
                        if let Some((id, _)) = waiter {
                            let t = self.tasks.get_mut(&id).unwrap();
                            t.wait = Wait::None;
                            t.state = State::Ready;
                            t.cpu.dar[0] = 0;
                        } else {
                            s.count = s.count.saturating_add(1);
                        }
                    } else {
                        code = 5;
                    }
                }
                0x39 => self.tick(),
                0x3a => {
                    self.date = r[1];
                    self.time = r[2];
                }
                0x3b => {
                    cpu.set_d(1, self.date);
                    cpu.set_d(2, self.time);
                    cpu.set_d(3, (self.ticks % 100) as u32);
                }
                0x3c => {
                    if r[6] != 0 {
                        self.block(Wait::Sleep, 0, r[6]);
                    } else if let Some(t) = self.tasks.get_mut(&self.current) {
                        t.state = State::Ready;
                        t.cpu = Context::capture(cpu);
                    }
                }
                0x3e | 0x41 => {
                    if self.timers.len() >= 1024 {
                        code = 0x45;
                    } else {
                        let timer = self.next_timer;
                        self.next_timer += 1;
                        self.timers.insert(
                            timer,
                            Timer {
                                task: self.current,
                                events: r[4],
                                due: self.ticks.saturating_add(u64::from(r[6])),
                                period: if op == 0x41 { u64::from(r[6]) } else { 0 },
                            },
                        );
                        cpu.set_d(1, timer);
                    }
                }
                0x40 => {
                    if self.timers.remove(&r[1]).is_none() {
                        code = 5;
                    }
                }
                0x204 | 0x20c | 0x21c => {
                    let mut files = tech2_emu::psos_files::PsosFiles::default();
                    files.enabled = true;
                    code = files.dispatch(op, cpu.sp(), bus).unwrap_or(0x2002);
                }
                0x208 => code = 0x2011,
                0x20d | 0x20e | 0x20f | 0x210 | 0x214 => code = 0x2002,
                _ => {
                    code = u32::MAX;
                    let error = format!("Unimplemented pSOS service {op:#x}");
                    self.error = Some(error.clone());
                    bus.failure_reason = Some(error);
                }
            }
        }
        cpu.set_d(0, code);
        if let Some(t) = self.tasks.get_mut(&self.current) {
            if t.state == State::Blocked {
                t.cpu = Context::capture(cpu);
            }
        }
        let force = self.tasks.get(&self.current).is_some_and(|t| {
            !matches!(t.state, State::Running | State::Ready) || t.suspended
        });
        self.schedule(cpu, bus, force);
        self.select_page(bus);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bus::ExecutionMode;
    fn fixture() -> (Kernel, CpuCore, Tech2Bus) {
        let mut bus = Tech2Bus::new(vec![0; 0x40000], vec![0; 0x200000], ExecutionMode::Fidelity);
        let header = 0x9000u32;
        bus.flash[header as usize..header as usize + 32].copy_from_slice(&[
            0, 0x10, 0, 0x40, 0x06, 0xff, 0xa2, 0x84, 0, 0, 0x39, 0x8c, 0x4e, 0x4b, 0x4e, 0x75,
            0x4e, 0x4c, 0x4e, 0x75, 0x4e, 0x4d, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0,
        ]);
        bus.write_long(0x100040, 0x100100);
        bus.write_long(0x100108, 0x100200);
        for (offset, value) in [
            (4, 0x150000),
            (8, 0x20000),
            (12, 4),
            (0x10, 32),
            (0x14, 32),
            (0x3c, 0x100800),
            (0x40, 0x400),
            (0x44, 0x400),
            (0x48, 0x2700),
            (0x54, 0x110000),
        ] {
            bus.write_long(0x100200 + offset, value);
        }
        bus.ram_mut(0x110000, 24).unwrap().copy_from_slice(&[
            0x48, 0x41, 0x13, 0xf0, 0x11, 0xb0, 0, 0x10, 3, 0, 0, 0x60, 6, 0, 0x33, 0xc1, 0, 0x10,
            4, 0, 0x48, 0x41, 0x4e, 0x75,
        ]);
        bus.write_byte(0x100301, 3);
        bus.write_byte(0x100302, 7);
        bus.write_byte(0x100303, 11);
        let mut cpu = CpuCore::new();
        cpu.set_cpu_type(m68k::CpuType::M68EC020);
        cpu.sr_mask = 0xa71f;
        cpu.set_sr(0x2700);
        cpu.set_sp(0x100f00);
        cpu.pc = header + 32;
        (Kernel::default(), cpu, bus)
    }
    fn call(k: &mut Kernel, c: &mut CpuCore, b: &mut Tech2Bus, op: u32, args: &[(usize, u32)]) {
        c.set_d(0, op);
        for &(n, value) in args {
            c.dar[n] = value;
        }
        assert!(k.trap(11, None, c, b));
    }
    #[test]
    fn only_exact_blank_kernel_is_adopted_and_root_page_comes_from_callback() {
        let (mut k, mut c, mut b) = fixture();
        k.before_instruction(&mut c, &mut b);
        assert!(k.active);
        assert_eq!(k.current, 0x10000);
        assert_eq!(c.pc, 0x100800);
        assert_eq!(c.get_sr(), 0x2700);
        assert_eq!(b.bank, 3);
        assert_eq!(b.u16(0x100400), Some(1));
        assert_eq!(b.u16(k.return_entry), Some(0x7200));
        let (mut k, mut c, mut b) = fixture();
        b.flash[0x9020] = 0x4e;
        k.before_instruction(&mut c, &mut b);
        assert!(!k.active);
        assert_eq!(k.kernel_entry, 0);
    }
    #[test]
    fn bad_callback_is_terminal_before_task_glue_allocation() {
        let (mut k, mut c, mut b) = fixture();
        b.write_word(0x110000, 0);
        let before = b.bytes(0x150000, 64).unwrap().to_vec();
        k.before_instruction(&mut c, &mut b);
        assert!(!k.active);
        assert!(b.failure_reason.as_ref().unwrap().contains("check_page"));
        assert_eq!(b.bytes(0x150000, 64).unwrap(), before);
    }
    #[test]
    fn blocking_queue_wakes_with_actual_words_and_restores_registers_and_bank() {
        let (mut k, mut c, mut b) = fixture();
        k.before_instruction(&mut c, &mut b);
        call(
            &mut k,
            &mut c,
            &mut b,
            0x24,
            &[(1, 0x54455354), (2, 2), (5, 4)],
        );
        let queue = c.dar[1];
        call(
            &mut k,
            &mut c,
            &mut b,
            1,
            &[(1, 0x4e415649), (2, 200), (3, 0x400), (4, 0)],
        );
        let nav = c.dar[1];
        call(
            &mut k,
            &mut c,
            &mut b,
            3,
            &[(1, nav), (2, 0x2000), (8, 0x100a00)],
        );
        assert_eq!(k.current, 0x10000);
        call(&mut k, &mut c, &mut b, 9, &[(1, 0x700), (2, 0)]);
        call(&mut k, &mut c, &mut b, 8, &[(1, 0), (2, 140)]);
        assert_eq!(k.current, nav);
        assert_eq!(c.pc, 0x100a00);
        assert_eq!(b.bank, 11);
        call(&mut k, &mut c, &mut b, 0x2a, &[(1, queue), (5, 0), (6, 3)]);
        assert_eq!(k.current, 0x10000);
        assert_eq!(b.bank, 3);
        assert_eq!(k.tasks[&nav].wait, Wait::Queue);
        call(
            &mut k,
            &mut c,
            &mut b,
            0x27,
            &[(1, queue), (2, 0x12345678), (3, 0xabcdef01), (4, 9), (5, 7)],
        );
        assert_eq!(k.current, nav);
        assert_eq!(&c.dar[2..6], &[0x12345678, 0xabcdef01, 9, 7]);
        assert_eq!(c.dar[0], 0);
        assert_eq!(b.bank, 11);
    }
    #[test]
    fn timeout_and_event_wait_do_not_fabricate_a_message() {
        let (mut k, mut c, mut b) = fixture();
        k.before_instruction(&mut c, &mut b);
        call(&mut k, &mut c, &mut b, 0x2d, &[(4, 0x10), (5, 0), (6, 2)]);
        assert_eq!(k.tasks[&0x10000].wait, Wait::Events);
        k.tick();
        assert_eq!(k.tasks[&0x10000].state, State::Blocked);
        k.tick();
        assert_eq!(k.tasks[&0x10000].cpu.dar[0], 1);
        assert_eq!(k.tasks[&0x10000].cpu.dar[4], 0);
        assert_eq!(k.tasks[&0x10000].state, State::Ready);
    }
    #[test]
    fn sleep_and_delayed_event_restore_blocked_task_after_due_tick() {
        let (mut k, mut c, mut b) = fixture();
        k.before_instruction(&mut c, &mut b);
        call(&mut k, &mut c, &mut b, 0x3e, &[(4, 0x20), (6, 2)]);
        call(&mut k, &mut c, &mut b, 0x2d, &[(4, 0x20), (5, 0), (6, 0)]);
        k.tick();
        assert_eq!(k.tasks[&0x10000].state, State::Blocked);
        k.tick();
        assert_eq!(k.tasks[&0x10000].state, State::Ready);
        assert_eq!(k.tasks[&0x10000].cpu.dar[4], 0x20);
        assert_eq!(k.tasks[&0x10000].cpu.dar[0], 0);
    }
    #[test]
    fn trap13_restores_real_exception_frame_and_trap12_remains_native() {
        let (mut k, mut c, mut b) = fixture();
        k.before_instruction(&mut c, &mut b);
        let sp = c.sp() - 12;
        c.set_sp(sp);
        b.write_word(sp, 0x2000);
        b.write_long(sp + 2, 0x100804);
        b.write_word(sp + 6, 0x2000);
        assert!(k.trap(13, None, &mut c, &mut b));
        assert_eq!(c.sp(), sp + 12);
        assert_eq!(c.get_sr(), 0x2000);
        assert_eq!(c.pc, 0x100804);
        assert!(!k.trap(12, None, &mut c, &mut b));
    }
    #[test]
    fn heap_and_unsupported_service_are_bounded() {
        let (mut k, mut c, mut b) = fixture();
        k.before_instruction(&mut c, &mut b);
        call(&mut k, &mut c, &mut b, 0x11, &[(1, 0), (3, u32::MAX)]);
        assert_eq!(c.dar[0], 0x21);
        assert_eq!(c.dar[8], 0);
        call(&mut k, &mut c, &mut b, 0xdead, &[]);
        assert_eq!(c.dar[0], u32::MAX);
        assert!(b.failure_reason.is_some());
    }
    #[test]
    fn context_and_host_tick_deadline_survive_checkpoint_rebase() {
        let (mut k, mut c, mut b) = fixture();
        k.before_instruction(&mut c, &mut b);
        k.last_host_tick = 900000;
        k.rebase(950000, 3000000);
        assert_eq!(k.last_host_tick, 2950000);
        let ctx = Context::capture(&c);
        let registers = c.dar;
        c.dar.fill(0xdeadbeef);
        c.set_sr(0);
        ctx.restore(&mut c);
        assert_eq!(c.dar, registers);
        assert_eq!(c.get_sr(), 0x2700);
    }
    #[test]
    fn zero_timeout_sleep_respects_interrupt_mask_preemption_and_priority() {
        let (mut k, mut c, mut b) = fixture();
        k.before_instruction(&mut c, &mut b);
        call(
            &mut k,
            &mut c,
            &mut b,
            1,
            &[(1, 0x574f524b), (2, 100), (3, 512), (4, 512), (5, 0)],
        );
        let worker = c.dar[1];
        call(&mut k, &mut c, &mut b, 3, &[(1, worker), (2, 0x2000), (8, 0x100800)]);
        // Lower-priority worker does not preempt ROOT on zero-timeout sleep (0x3c with D6=0).
        c.set_sr(0x2000);
        call(&mut k, &mut c, &mut b, 0x3c, &[(6, 0)]);
        assert_eq!(k.current, 0x10000);
        assert_eq!(k.tasks[&0x10000].state, State::Ready);
        // Raising worker priority above ROOT still honors interrupt mask (SR & 0x700 != 0).
        call(&mut k, &mut c, &mut b, 8, &[(1, worker), (2, 255)]);
        k.tasks.get_mut(&0x10000).unwrap().priority = 200;
        c.set_sr(0x2700);
        call(&mut k, &mut c, &mut b, 0x3c, &[(6, 0)]);
        assert_eq!(k.current, 0x10000);
        // Unmasking SR allows higher-priority worker to switch in on 0x3c (D6=0).
        c.set_sr(0x2000);
        call(&mut k, &mut c, &mut b, 0x3c, &[(6, 0)]);
        assert_eq!(k.current, worker);
        // Self-suspend forces reschedule even when SR is masked.
        c.set_sr(0x2700);
        call(&mut k, &mut c, &mut b, 6, &[(1, 0)]);
        assert_eq!(k.current, 0x10000);
    }
}
