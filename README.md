# 🦅 redKiteOS

> **An experimental x86_64 research operating system written in bare-metal Rust (`no_std`).**  
> *Project Status: Archived.*

---

## 📌 Project Purpose & Disclaimer

**redKiteOS** is an experimental hobby operating system kernel targeting the x86_64 architecture. 

The primary objective of this project was **hands-on exploration and learning of low-level systems programming in Rust on bare-metal hardware**. It was developed without relying on the standard library runtime (`#![no_std]`), existing host operating system syscalls, or high-level platform abstractions.

The project served as an intensive engineering sandbox to investigate how Rust's ownership model, type safety, inline assembly, and zero-cost abstractions interact directly with physical hardware: CPU control registers, memory management units (MMU), hardware interrupt controllers, PCI buses, and I/O ports.

> **Notice:** This repository is preserved in an **archived state**. Development has concluded, and the codebase stands as an educational monument to bare-metal OS development challenges.

---

## 🏛 Architectural Overview

Despite being an educational research vehicle, **redKiteOS** implements a vertical slice of a complete operating system stack from silicon to user applications:

### 1. Hardware Abstraction Layer (HAL & x86_64 Core)
* **Bootstrap:** Boots into 64-bit Long Mode via the pure-Rust `bootloader` (v0.9).
* **Segmentation:** Custom Global Descriptor Table (**GDT**) defining Kernel/User code and data selectors (Ring 0 & Ring 3), alongside a Task State Segment (**TSS**) configured with an Interrupt Stack Table (**IST**).
* **Interrupt Infrastructure:** Complete Interrupt Descriptor Table (**IDT**) handling hardware exceptions (`#DE`, `#UD`, `#GP`, `#PF`, and dedicated IST stack for `#DF` Double Faults).
* **Legacy Peripheral Control:** Dual **8259 PIC** cascade mapping, programmable interval timer (**PIT 8254**) frequency synthesis, and **CMOS RTC** timekeeping.
* **Input Drivers:** PS/2 controller driver handling Set 1 keyboard scancodes and 3-byte packet stream mouse decoding.
* **Serial Telemetry:** 16550 UART (**COM1**) serial communication for remote debugging and bidirectional binary data injection.
* **SIMD & Coprocessor Control:** Hardware configuration via `CR0`/`CR4` to initialize SSE and floating-point units.

### 2. Memory Management (MMU)
* **Paging:** 4-level paging (PML4, PDPT, PD, PT) leveraging the `x86_64` architecture abstractions.
* **Physical Frame Allocator:** BootInfo-backed usable physical memory tracker allocating individual 4 KiB frames.
* **Kernel Heap:** Dynamic heap allocator backed by a 4 MiB virtual memory region initialized via `linked_list_allocator`.

### 3. Graphics Subsystem & Window Compositor
* **Display Driver:** Bochs VBE Graphics Adapter (**BGA**) running in 800×600 True Color (32 bits per pixel) linear framebuffer mode.
* **Compositor Architecture:** Double/triple-buffering pipeline (Background cache $\rightarrow$ RAM Backbuffer $\rightarrow$ PCI VRAM Linear Framebuffer).
* **Window Server:**
  * Asynchronous 60 FPS compositor with selective **dirty-rectangle invalidation** (`Rect` unions).
  * Draggable, focusable, floating windows with window decor and close controls.
  * **Magnetic Edge Snapping:** Smooth geometric snapping to the top taskbar baseline and adjacent window boundaries.
  * High-performance L-shaped shadow boundaries and alpha-blending.
  * Monospaced text rendering using an embedded 8×8 bitmapped font engine (`font8x8`).
  * Built-in terminal emulator parsing ANSI CSI escape sequences (VT100 color support).

### 4. Storage & Virtual File System (KiteFS)
* **Block Device Driver:** Parallel ATA (IDE) storage driver operating in 28-bit LBA Polled I/O (PIO) mode.
* **KiteFS Custom Filesystem:** Unix-like filesystem structure featuring a dedicated superblock, inode allocation bitmap, direct data blocks, and indirect block pointers.
* **POSIX-Compliant VFS:** Hierarchical path resolution, symbolic links with recursion depth limits (`ELOOP`), directory enumeration, and standard octal permission bits (`0o755`, `0o644`, `0o660`).
* **Device Nodes:** Integration of special virtual devices (`/dev/null`, `/dev/fb0`, `/dev/mouse`, `/dev/kbd`, `/dev/ata0`).

### 5. Network Stack (L2 through L7)
* **NIC Driver:** Realtek **RTL8139** network interface controller initialization over PCI configuration space.
* **Packet Framing:** Ethernet II frame encapsulation with RFC 1071 internet checksum generation.
* **Core Protocols:** ARP resolution, IPv4 packet handling, and ICMP Echo Request/Reply (`ping`).
* **Transport & App Layers:** UDP datagram transmission, custom DNS query resolver, raw TCP 3-way handshake prototype, and a bare-metal HTTP client.

### 6. Userspace, Shell & Binary Loading
* **Interactive Shell (`sh.rs`):** POSIX.1-2008 compliant command processor inspired by ZSH:
  * Dynamic prompt formatting (`$PS1`).
  * Command pipelines (`|`), logical chaining (`&&`, `||`), and file redirection (`>`, `>>`).
  * Environment variable management, custom command aliases (`alias`), and execution `history`.
  * Inline mathematical expression parser with operator precedence.
  * Background job execution via `&`.
  * Privilege elevation model (`su` / `user` PAM mock).

---

## ⚖️ Engineering Trade-offs: Strengths & Weaknesses

### 🌟 Key Advantages & Achievements

1. **Pure Rust Systems Programming:**  
   The entire OS is implemented strictly in Rust with minimal dependencies, proving the feasibility of type-safe systems programming without an underlying runtime.
2. **Broad Vertical Architecture:**  
   In just a few thousand lines of code, the system integrates hardware initialization, interrupt handling, disk storage, an IP networking stack, an asynchronous GUI compositor, and an interactive shell.
3. **Optimized Software Rendering:**  
   Identifying and eliminating architectural bottlenecks—such as discarding software-emulated floating-point math (`sqrtf` in `soft-float`) and switching to dirty-rectangle clipping—enabled smooth desktop interactivity even within unaccelerated virtual machines.
4. **Hands-On Triage of Low-Level Hardware Traps:**  
   The project successfully resolved critical hardware traps inherent to x86_64 OS development:
   * Coprocessor faults (`#UD`) caused by uninitialized SSE state registers (`CR0`/`CR4`).
   * Memory corruption triggered by PCI Bus Master DMA accessing unmapped virtual addresses instead of contiguous physical frames.
   * CPU protection faults triggered by **SMAP/SMEP** hardware security enforcement during kernel access to user memory.
   * Segment overlap resolution during ELF64 virtual memory mapping.

---

### ⚠️ Known Architectural Flaws & Limitations (Why the Project is Frozen)

*To preserve academic honesty, the following design compromises must be highlighted:*

1. **Single Shared Address Space (Lack of Per-Process CR3 Isolation):**  
   The kernel and user applications share a single global page table (`MAPPER`). User processes do not run in isolated virtual address spaces with distinct `CR3` roots. This creates the risk of address collisions and provides no hardware-level isolation between concurrent processes.
2. **Absence of True Preemptive Multitasking for Userspace:**  
   User execution operates via a synchronous trampoline mechanism (`enter_userspace` / `iretq` / `sys_exit`). Launching a foreground binary effectively suspends the shell thread rather than scheduling it concurrently as an independent, preemptible task.
3. **Insecure Kernel Stack Handling in `syscall_entry`:**  
   The x86_64 `syscall` instruction does not automatically switch stack pointers (`RSP`). The kernel naively pushes registers onto the user-supplied stack inside Ring 0 rather than switching to an isolated per-CPU kernel stack via `swapgs`. This compromises kernel stack integrity and necessitated disabling hardware **SMAP** protections in `CR4`.
4. **Synchronous Polled I/O Storage (ATA PIO Mode):**  
   Disk access relies entirely on busy-wait polling of legacy IDE I/O ports. There is no support for Bus Master DMA (PRD tables), AHCI (SATA), or NVMe, resulting in high CPU usage during disk I/O.
5. **Fragile Custom Filesystem (KiteFS):**  
   The filesystem has no journaling, crash-recovery mechanisms, or dynamic resizing. Storage capacity is constrained by a hardcoded maximum of 64 inodes and 200 data blocks.
6. **Simplified Network DMA Handling:**  
   The RTL8139 network driver lacks an IOMMU-backed dynamic ring buffer descriptor pool. Some network responses rely on software fallbacks to bypass virtual machine bus synchronization issues.
7. **No Standard C Library (Libc):**  
   User applications must link against the internal, minimal `libkite` runtime. Standard Rust or C binaries cannot be ported or executed without implementing a complete C library layer (e.g., `relibc` or `musl`).

---

## 🛠 Building & Running (Historical Reference)

### Prerequisites
* Host OS: **Linux** (tested on Arch Linux).
* Toolchain: Rust `nightly`, `rust-src`, `llvm-tools-preview`, `bootimage`, and `qemu-system-x86_64`.

### Environment Setup
```bash
rustup toolchain install nightly
rustup default nightly
rustup component add rust-src llvm-tools-preview
cargo install bootimage
```

### Launching via QEMU
```bash
# Clone the repository
git clone https://github.com/avuzuku/redKiteOS.git
cd redKiteOS

# Build bootable raw disk image and execute in QEMU
cargo run
```

---

## 📄 License

This project is open-source software licensed under the **MIT License**.  
Copyright (c) **2026 Aliaksandr Vazheika**.

See the [LICENSE](LICENSE) file for complete terms.