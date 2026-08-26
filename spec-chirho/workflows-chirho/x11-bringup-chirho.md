<!-- For God so loved the world, that he gave his only begotten Son, that whosoever believeth in him should not perish, but have everlasting life. — John 3:16 (KJV) -->

# X11 Bring-up Workflow Chirho

The material rootfs `/etc/profile` invokes
`/usr/local/sbin/start-lineluya-desktop-chirho.sh`, which is the single launcher
for Xorg and its clients. A rootfs-owned atomic directory guard prevents a
second launcher; VFS does not synthesize profile or Xorg configuration content.
`x11_bringup_chirho.rs` owns readiness and the waiting-client queue;
`net_core_chirho.rs` reports socket events and parks clients; the epoll syscall
dispatch reports Xorg event-loop entry.

```mermaid
flowchart TD
    profile_chirho[Material rootfs profile invokes launcher]
    guard_chirho{Atomic launcher guard acquired?}
    launcher_exit_chirho[Existing launcher exits without duplicate start]
    launch_chirho[Launcher starts Xorg once]
    probe_chirho[Repository-built xgears probe validates authentic XCB reply]
    wm_probe_chirho[Probe validates twm owns SubstructureRedirect]
    clients_chirho[Launcher starts xterm and xgears once]
    bind_chirho[Xorg binds display socket]
    bind_hook_chirho[sys_bind_chirho calls on_display_socket_bound_chirho]
    connect_chirho[Client calls sys_connect_chirho]
    connect_ok_chirho{AF_UNIX connect succeeds?}
    x11_path_chirho{X11 display path?}
    accepting_chirho{xorg_accepting_clients_chirho?}
    register_chirho[register_waiting_client_chirho]
    park_chirho[block_current_chirho marks client Sleeping and schedules]
    epoll_chirho[Xorg calls epoll_wait or epoll_pwait]
    identity_chirho{Executable basename is Xorg?}
    ready_chirho[Mark Xorg accepting]
    drain_chirho[Drain waiting PIDs on every Xorg epoll wait]
    wake_chirho[unblock_task_chirho marks clients Ready and requeues]
    retry_chirho[Woken client retries AF_UNIX connect]
    return_chirho[Return connect result to userspace]
    epoll_return_chirho[Return epoll result to Xorg]

    profile_chirho --> guard_chirho
    guard_chirho -- no --> launcher_exit_chirho
    guard_chirho -- yes --> launch_chirho
    launch_chirho --> bind_chirho --> bind_hook_chirho
    launch_chirho --> probe_chirho --> connect_chirho
    probe_chirho --> wm_probe_chirho --> clients_chirho
    clients_chirho --> connect_chirho
    connect_chirho --> connect_ok_chirho
    connect_ok_chirho -- yes --> return_chirho
    connect_ok_chirho -- no --> x11_path_chirho
    x11_path_chirho -- no --> return_chirho
    x11_path_chirho -- yes --> accepting_chirho
    accepting_chirho -- yes --> return_chirho
    accepting_chirho -- no --> register_chirho --> park_chirho
    launch_chirho --> epoll_chirho --> identity_chirho
    identity_chirho -- no --> epoll_return_chirho
    identity_chirho -- yes --> ready_chirho --> drain_chirho --> wake_chirho
    drain_chirho --> epoll_return_chirho
    wake_chirho --> retry_chirho --> return_chirho
```

The readiness log is one-shot, but the waiting-PID drain is not latched. A
client may park after Xorg's first wait, so every later Xorg wait must drain the
queue. `X11_READY_CHIRHO` remains only as a temporary console-trace input; it is
not readiness authority.

Production proof gate: the KVM serial log must show Xorg's executable-identified
event-loop entry, authentic XCB setup success, twm ownership, and successful
wake/retry for any client that parked early. If Xorg dies before readiness, the
parked-client failure path must wake waiters so the launcher's bounded teardown
can report failure rather than sleeping forever.
