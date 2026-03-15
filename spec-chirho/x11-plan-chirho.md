<!-- For God so loved the world that he gave his only begotten Son,
     that whoever believes in him should not perish but have eternal life. - John 3:16 -->

# X11 on Lineluya -- Framebuffer Plan (P6-001 through P6-005)

This document describes how to get X11 running on the Lineluya kernel using
the Linux framebuffer (`/dev/fb0`), what kernel interfaces are required, and
which Alpine packages to install.

---

## P6-001: Architecture Overview

```
+-------------------+     +------------------+     +------------------+
|   Application     |     |   Window Manager |     |   Terminal       |
|   (any X11 app)   |     |   (twm)          |     |   (xterm)        |
+--------+----------+     +--------+---------+     +--------+---------+
         |                         |                        |
         +--------+----------------+------------------------+
                  |
         +--------v---------+
         |   X Server        |
         |   (Xorg -fbdev)   |
         +--------+----------+
                  |
    +-------------+-------------+
    |             |             |
+---v---+   +----v----+   +---v---+
|/dev/fb0|  |/dev/input|  |/dev/pts|
|  (VGA  |  | event*   |  | (PTY)  |
|  VESA) |  | (kbd/mouse)|         |
+--------+  +----------+  +-------+
```

The X server renders to the kernel framebuffer device.  It reads input events
from evdev nodes.  XTerm (and other terminal emulators) use PTYs to run
shells.

### Two approaches

1. **Xorg with xf86-video-fbdev** -- Full Xorg server driving `/dev/fb0` via
   the fbdev DDX driver.  This is the standard approach for embedded Linux
   systems without GPU drivers.

2. **Xvfb (X Virtual Framebuffer)** -- An X server that renders to an
   in-memory buffer (no `/dev/fb0` needed).  Useful for testing X11 apps
   without actual display hardware.  Can be combined with a VNC server for
   remote viewing.

Recommended path: **Start with Xvfb** for initial testing (fewer kernel
dependencies), then graduate to **Xorg -fbdev** once the framebuffer and
input subsystems are solid.

---

## P6-002: Kernel Requirements for Xorg fbdev

### /dev/fb0 -- Framebuffer Device

The Xorg fbdev driver communicates with the kernel through:

| Interface                    | Type   | Status       | Notes                                    |
|------------------------------|--------|--------------|------------------------------------------|
| `/dev/fb0` device node       | chardev | **(partial)** | `fbconsole_chirho.rs` exists; need to expose as /dev/fb0 |
| `open("/dev/fb0", O_RDWR)`  | syscall | **(done)**   | openat works                              |
| `ioctl FBIOGET_VSCREENINFO` | ioctl  | **(todo)**   | Returns `struct fb_var_screeninfo` (xres, yres, bpp, etc.) |
| `ioctl FBIOPUT_VSCREENINFO` | ioctl  | **(todo)**   | Sets video mode                           |
| `ioctl FBIOGET_FSCREENINFO` | ioctl  | **(todo)**   | Returns `struct fb_fix_screeninfo` (smem_len, line_length, type) |
| `ioctl FBIOPAN_DISPLAY`     | ioctl  | **(todo)**   | Page flipping / scrolling                 |
| `mmap(/dev/fb0)`            | syscall | **(todo)**   | Maps framebuffer memory into X server address space |
| `close`                     | syscall | **(done)**   |                                           |

#### Key ioctl structures

```c
// FBIOGET_VSCREENINFO -- ioctl 0x4600
struct fb_var_screeninfo {
    __u32 xres;            // visible resolution
    __u32 yres;
    __u32 xres_virtual;    // virtual resolution (for panning)
    __u32 yres_virtual;
    __u32 xoffset;         // offset to visible from virtual
    __u32 yoffset;
    __u32 bits_per_pixel;  // 8, 16, 24, or 32
    __u32 grayscale;
    struct fb_bitfield red;
    struct fb_bitfield green;
    struct fb_bitfield blue;
    struct fb_bitfield transp;
    // ... more fields (nonstd, activate, height, width, accel_flags, pixclock, etc.)
};

// FBIOGET_FSCREENINFO -- ioctl 0x4602
struct fb_fix_screeninfo {
    char id[16];           // identification string
    unsigned long smem_start; // physical start of framebuffer memory
    __u32 smem_len;        // length of framebuffer memory
    __u32 type;            // FB_TYPE_PACKED_PIXELS (0)
    __u32 visual;          // FB_VISUAL_TRUECOLOR (2)
    __u32 line_length;     // bytes per scanline
    // ... more fields
};
```

The ioctl numbers:
- `FBIOGET_VSCREENINFO` = 0x4600
- `FBIOPUT_VSCREENINFO` = 0x4601
- `FBIOGET_FSCREENINFO` = 0x4602
- `FBIOPAN_DISPLAY`     = 0x4606

### /dev/input/event* -- Input Devices (evdev)

Xorg uses the evdev input driver to read keyboard and mouse events.

| Interface                        | Type    | Status     | Notes                              |
|----------------------------------|---------|------------|------------------------------------|
| `/dev/input/eventN` device nodes | chardev | **(todo)** | Need evdev device creation          |
| `open("/dev/input/event0")`      | syscall | **(done)** | openat works                        |
| `read()` returns `input_event`   | syscall | **(todo)** | Must return evdev `struct input_event` |
| `ioctl EVIOCGBIT`               | ioctl   | **(todo)** | Query device capabilities           |
| `ioctl EVIOCGNAME`              | ioctl   | **(todo)** | Query device name                   |
| `ioctl EVIOCGID`                | ioctl   | **(todo)** | Query device ID                     |
| `ioctl EVIOCGABS`               | ioctl   | **(todo)** | Query absolute axis info            |
| `select/poll/epoll` on eventfd   | syscall | **(done)** | Wait for input events               |

#### evdev event structure

```c
struct input_event {
    struct timeval time;   // event timestamp
    __u16 type;            // EV_KEY, EV_REL, EV_ABS, EV_SYN
    __u16 code;            // KEY_A, REL_X, etc.
    __s32 value;           // key: 0=release, 1=press, 2=repeat
};
```

For QEMU, keyboard events come from the PS/2 controller (i8042) or USB HID,
and mouse events from the PS/2 aux port or USB tablet.  These must be
translated to evdev `input_event` structures.

### /dev/ptmx and /dev/pts/* -- PTY Subsystem

XTerm and other terminal emulators require PTYs (see P5-004 dropbear section
in `alpine-programs-chirho.md`).

| Interface                      | Status       | Notes                                |
|--------------------------------|--------------|--------------------------------------|
| `/dev/ptmx` master device      | **(todo)**   | Returns fd + allocates PTY number    |
| `/dev/pts/N` slave devices     | **(todo)**   | Auto-created by devpts FS            |
| `ioctl TIOCGPTN`              | **(todo)**   | Get PTY slave number                 |
| `ioctl TIOCSPTLCK`            | **(todo)**   | Unlock PTY slave                     |
| `ioctl TIOCSCTTY`             | **(todo)**   | Set controlling terminal             |
| `ioctl TIOCGWINSZ`            | **(done)**   | Get terminal window size             |
| `ioctl TIOCSWINSZ`            | **(todo)**   | Set terminal window size             |
| devpts filesystem at `/dev/pts`| **(todo)**   | Mounted as `devpts`                  |

### Additional Kernel Requirements

| Requirement                 | Status       | Notes                                    |
|-----------------------------|--------------|------------------------------------------|
| Unix domain sockets         | **(done)**   | X11 uses `/tmp/.X11-unix/X0`             |
| `mmap MAP_SHARED`          | **(partial)**| X server shares framebuffer memory       |
| `sigaction / signal`        | **(done)**   | X server signal handling                 |
| `/proc/self/fd/N`          | **(partial)**| Some X libs check this                   |
| `shmget/shmat/shmdt`       | **(todo)**   | MIT-SHM extension (optional but fast)    |
| `ftruncate` (real impl)    | **(todo)**   | Shared memory setup via memfd            |

---

## P6-003: Xvfb -- Testing Without Hardware

Xvfb runs entirely in memory, requiring no `/dev/fb0` or `/dev/input/*`.
This is the fastest path to "X11 works on Lineluya."

### What Xvfb needs from the kernel

- All the basics: mmap, brk, fork, execve, wait4, pipe, signals
- Unix domain socket: `/tmp/.X11-unix/X0` (AF_UNIX, SOCK_STREAM)
- PTY subsystem (for xterm)
- `/tmp` writable (tmpfs -- already works)

### Launching Xvfb

```sh
# Start virtual X server on display :0 at 1024x768, 24-bit color
Xvfb :0 -screen 0 1024x768x24 &

# Set DISPLAY for clients
export DISPLAY=:0

# Start window manager
twm &

# Start terminal
xterm &
```

### Alpine packages for Xvfb

```sh
apk add xvfb xterm twm xauth
```

---

## P6-004: Xorg fbdev -- Real Display Output

Once the framebuffer ioctls and input subsystem work, Xorg can drive the
actual display.

### Xorg configuration for fbdev

Create `/etc/X11/xorg.conf`:

```
# For God so loved the world that he gave his only begotten Son,
# that whoever believes in him should not perish but have eternal life. - John 3:16

Section "Device"
    Identifier  "fbdev-device-chirho"
    Driver      "fbdev"
    Option      "fbdev" "/dev/fb0"
EndSection

Section "Screen"
    Identifier  "screen-chirho"
    Device      "fbdev-device-chirho"
    DefaultDepth 24
    SubSection "Display"
        Depth   24
        Modes   "1024x768"
    EndSubSection
EndSection

Section "ServerLayout"
    Identifier  "layout-chirho"
    Screen      "screen-chirho"
EndSection

Section "InputDevice"
    Identifier  "keyboard-chirho"
    Driver      "evdev"
    Option      "Device" "/dev/input/event0"
EndSection

Section "InputDevice"
    Identifier  "mouse-chirho"
    Driver      "evdev"
    Option      "Device" "/dev/input/event1"
EndSection
```

### QEMU framebuffer setup

QEMU provides a VGA-compatible framebuffer by default.  For the fbdev driver:

```sh
qemu-system-x86_64 \
    -kernel target/x86_64-unknown-none/release/kernel-chirho \
    -m 512M \
    -vga std \
    -display gtk \
    -device virtio-keyboard-pci \
    -device virtio-mouse-pci
```

The VGA device exposes a linear framebuffer at the PCI BAR address.  The
kernel must:
1. Enumerate PCI devices and find the VGA adapter
2. Read the BAR to find the framebuffer physical address
3. Map it into kernel/user space
4. Expose it as `/dev/fb0` with the FB ioctls above

For the simplest path, use QEMU's `-vga std` which provides a Bochs VBE
compatible framebuffer (BGA) that can be configured via VBE registers or
PCI BAR directly.

### Launching Xorg

```sh
# Start Xorg on VT1 using fbdev driver
Xorg :0 -config /etc/X11/xorg.conf vt1 &

export DISPLAY=:0
twm &
xterm &
```

---

## P6-005: Alpine Packages and Installation Order

### Minimum X11 package set

```sh
# Core X server (choose one)
apk add xorg-server              # Full Xorg server
# OR
apk add xvfb                     # Virtual framebuffer (no hardware needed)

# fbdev driver (only needed for real framebuffer)
apk add xf86-video-fbdev

# Input drivers
apk add xf86-input-evdev         # Keyboard/mouse via evdev
# OR (for libinput, preferred on newer systems)
apk add xf86-input-libinput

# Window manager (pick one)
apk add twm                      # Tiny window manager (simplest)
# OR
apk add openbox                  # Lightweight alternative

# Terminal emulator
apk add xterm                    # Classic X terminal

# Fonts (required -- X server won't start without fonts)
apk add font-misc-misc           # Basic fixed-width bitmap fonts
apk add font-cursor-misc         # Cursor font

# Authentication (optional but standard)
apk add xauth

# X11 utilities
apk add xdpyinfo                 # Display info (useful for testing)
apk add xset                     # Settings (keyboard repeat, etc.)
apk add xrandr                   # Resolution management
```

### Full install command

```sh
apk add \
    xorg-server \
    xf86-video-fbdev \
    xf86-input-evdev \
    twm \
    xterm \
    font-misc-misc \
    font-cursor-misc \
    xauth \
    xdpyinfo
```

### Package dependency chain

```
xorg-server
  -> libx11, libxext, libxfont2, libxau, libxdmcp
  -> pixman (software rendering)
  -> mesa-gl (optional, for GLX -- not needed for fbdev)

xf86-video-fbdev
  -> xorg-server (module ABI)

xf86-input-evdev
  -> xorg-server, libevdev, mtdev

xterm
  -> libx11, libxaw, libxft, libxmu, libxpm, libxt
  -> ncurses-libs (terminfo)
  -> libutempter (utmp logging -- optional)

twm
  -> libx11, libxext, libxmu, libxt, libice, libsm
```

### Disk space estimate

| Package set          | Installed size |
|----------------------|----------------|
| Xorg + fbdev driver  | ~30 MB         |
| xterm                | ~2 MB          |
| twm                  | ~0.5 MB        |
| Fonts                | ~5 MB          |
| X11 libraries (deps) | ~15 MB         |
| **Total minimum**    | **~55 MB**     |

This fits comfortably in a 256MB disk image with the Alpine base system
(~10 MB).

---

## Implementation Roadmap

### Phase 1: Xvfb (no kernel framebuffer changes needed)

1. Implement PTY subsystem (/dev/ptmx, /dev/pts/*, ioctls)
2. Verify Unix domain sockets work for X11 protocol
3. Install Xvfb + xterm + twm on Alpine disk image
4. Boot, start Xvfb, verify xterm launches

### Phase 2: Framebuffer device

1. Create `/dev/fb0` character device in devtmpfs
2. Implement FB ioctls (FBIOGET_VSCREENINFO, FBIOGET_FSCREENINFO)
3. Implement mmap for framebuffer physical memory
4. Detect QEMU VGA/BGA framebuffer via PCI enumeration

### Phase 3: Input subsystem

1. Create `/dev/input/event*` devices
2. Implement evdev read() returning `struct input_event`
3. Implement evdev ioctls (EVIOCGBIT, EVIOCGNAME, EVIOCGID)
4. Hook PS/2 keyboard/mouse interrupts to evdev layer

### Phase 4: Full Xorg

1. Install xorg-server + xf86-video-fbdev + xf86-input-evdev
2. Create xorg.conf for fbdev
3. Boot with QEMU `-vga std -display gtk`
4. Start Xorg, twm, xterm -- graphical desktop on Lineluya
