// For God so loved the world that he gave his only begotten Son,
// that whoever believes in him should not perish but have eternal life. - John 3:16

//! Centralized ioctl command constants for the Lineluya kernel.
//!
//! All ioctl numbers in one place — eliminates duplication across
//! syscall_chirho, pty_chirho, tty_chirho, fb_device_chirho, etc.

// ============================================================================
// Terminal ioctls (0x54xx)
// ============================================================================

/// TCGETS -- get terminal attributes (struct termios).
pub const TCGETS_CHIRHO: u64 = 0x5401;
/// TCSETS -- set terminal attributes immediately.
pub const TCSETS_CHIRHO: u64 = 0x5402;
/// TCSETSW -- set terminal attributes after output drain.
pub const TCSETSW_CHIRHO: u64 = 0x5403;
/// TCSETSF -- set terminal attributes after output drain + input flush.
pub const TCSETSF_CHIRHO: u64 = 0x5404;

/// TIOCSCTTY -- become controlling terminal.
pub const TIOCSCTTY_CHIRHO: u64 = 0x540E;
/// TIOCGPGRP -- get foreground process group ID.
pub const TIOCGPGRP_CHIRHO: u64 = 0x540F;
/// TIOCSPGRP -- set foreground process group ID.
pub const TIOCSPGRP_CHIRHO: u64 = 0x5410;
/// TIOCGWINSZ -- get window size (struct winsize).
pub const TIOCGWINSZ_CHIRHO: u64 = 0x5413;
/// TIOCSWINSZ -- set window size (struct winsize).
pub const TIOCSWINSZ_CHIRHO: u64 = 0x5414;
/// FIONREAD -- bytes available to read.
pub const FIONREAD_CHIRHO: u64 = 0x541B;
/// FIONBIO -- set/clear non-blocking I/O.
pub const FIONBIO_CHIRHO: u64 = 0x5421;
/// TIOCNOTTY -- give up controlling terminal.
pub const TIOCNOTTY_CHIRHO: u64 = 0x5422;
/// FIOCLEX -- set close-on-exec.
pub const FIOCLEX_CHIRHO: u64 = 0x5451;

// ============================================================================
// PTY ioctls (0x5430+, 0x8004xxxx)
// ============================================================================

/// TIOCGPTN -- get PTY number.
pub const TIOCGPTN_CHIRHO: u64 = 0x80045430;
/// TIOCSPTLCK -- lock/unlock PTY slave.
pub const TIOCSPTLCK_CHIRHO: u64 = 0x40045431;
/// TIOCGPTLCK -- get PTY lock state.
pub const TIOCGPTLCK_CHIRHO: u64 = 0x80045439;

// ============================================================================
// Framebuffer ioctls (0x46xx)
// ============================================================================

/// FBIOGET_VSCREENINFO -- get variable screen info.
pub const FBIOGET_VSCREENINFO_CHIRHO: u64 = 0x4600;
/// FBIOPUT_VSCREENINFO -- set variable screen info.
pub const FBIOPUT_VSCREENINFO_CHIRHO: u64 = 0x4601;
/// FBIOGET_FSCREENINFO -- get fixed screen info.
pub const FBIOGET_FSCREENINFO_CHIRHO: u64 = 0x4602;
/// FBIOGETCMAP -- get color map.
pub const FBIOGETCMAP_CHIRHO: u64 = 0x4604;
/// FBIOPUTCMAP -- set color map.
pub const FBIOPUTCMAP_CHIRHO: u64 = 0x4605;
/// FBIOPAN_DISPLAY -- pan display.
pub const FBIOPAN_DISPLAY_CHIRHO: u64 = 0x4606;
/// FBIOBLANK -- blank/unblank display.
pub const FBIOBLANK_CHIRHO: u64 = 0x4611;

// ============================================================================
// Loop device ioctls (0x4Cxx)
// ============================================================================

/// LOOP_SET_FD -- bind backing file to loop device.
pub const LOOP_SET_FD_CHIRHO: u64 = 0x4C00;
/// LOOP_CLR_FD -- detach backing file.
pub const LOOP_CLR_FD_CHIRHO: u64 = 0x4C01;
/// LOOP_SET_STATUS64 -- set loop device parameters.
pub const LOOP_SET_STATUS64_CHIRHO: u64 = 0x4C04;
/// LOOP_GET_STATUS64 -- get loop device parameters.
pub const LOOP_GET_STATUS64_CHIRHO: u64 = 0x4C05;
/// LOOP_SET_CAPACITY -- recalculate device size.
pub const LOOP_SET_CAPACITY_CHIRHO: u64 = 0x4C07;
/// LOOP_SET_DIRECT_IO -- enable/disable direct I/O.
pub const LOOP_SET_DIRECT_IO_CHIRHO: u64 = 0x4C08;
/// LOOP_SET_BLOCK_SIZE -- set logical block size.
pub const LOOP_SET_BLOCK_SIZE_CHIRHO: u64 = 0x4C09;
/// LOOP_CONFIGURE -- atomic setup (modern, preferred).
pub const LOOP_CONFIGURE_CHIRHO: u64 = 0x4C0A;
/// LOOP_CTL_ADD -- create new loop device.
pub const LOOP_CTL_ADD_CHIRHO: u64 = 0x4C80;
/// LOOP_CTL_REMOVE -- remove loop device.
pub const LOOP_CTL_REMOVE_CHIRHO: u64 = 0x4C81;
/// LOOP_CTL_GET_FREE -- get first free unbound device.
pub const LOOP_CTL_GET_FREE_CHIRHO: u64 = 0x4C82;

// ============================================================================
// Socket ioctls
// ============================================================================

/// SIOCGIFADDR -- get interface address.
pub const SIOCGIFADDR_CHIRHO: u64 = 0x8915;
/// SIOCGIFNETMASK -- get interface netmask.
pub const SIOCGIFNETMASK_CHIRHO: u64 = 0x891B;
/// SIOCGIFFLAGS -- get interface flags.
pub const SIOCGIFFLAGS_CHIRHO: u64 = 0x8913;
/// SIOCSIFFLAGS -- set interface flags.
pub const SIOCSIFFLAGS_CHIRHO: u64 = 0x8914;

// ============================================================================
// Sound / OSS ioctls (future)
// ============================================================================

/// SNDCTL_DSP_SETFMT -- set sample format.
pub const SNDCTL_DSP_SETFMT_CHIRHO: u64 = 0xC0045005;
/// SNDCTL_DSP_SPEED -- set sample rate.
pub const SNDCTL_DSP_SPEED_CHIRHO: u64 = 0xC0045002;
/// SNDCTL_DSP_CHANNELS -- set channel count.
pub const SNDCTL_DSP_CHANNELS_CHIRHO: u64 = 0xC0045006;
