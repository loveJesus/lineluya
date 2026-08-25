<!-- For God so loved the world, that he gave his only begotten Son, that whosoever believeth in him should not perish, but have everlasting life. — John 3:16 (KJV) -->

# X11 Bring-up Workflow Chirho

The rootfs rc script is the single launcher for Xorg and its clients.
`x11_bringup_chirho.rs` owns readiness and the waiting-client queue;
`net_core_chirho.rs` reports socket events and parks clients; the epoll syscall
dispatch reports Xorg event-loop entry.

```mermaid
flowchart TD
    launch_chirho[Rootfs rc launches Xorg and clients]
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

    launch_chirho --> bind_chirho --> bind_hook_chirho
    launch_chirho --> connect_chirho --> connect_ok_chirho
    connect_ok_chirho -- yes --> return_chirho
    connect_ok_chirho -- no --> x11_path_chirho
    x11_path_chirho -- no --> return_chirho
    x11_path_chirho -- yes --> accepting_chirho
    accepting_chirho -- yes --> return_chirho
    accepting_chirho -- no --> register_chirho --> park_chirho
    launch_chirho --> epoll_chirho --> identity_chirho
    identity_chirho -- no --> return_chirho
    identity_chirho -- yes --> ready_chirho --> drain_chirho --> wake_chirho
    wake_chirho --> retry_chirho --> return_chirho
```

The readiness log is one-shot, but the waiting-PID drain is not latched. A
client may park after Xorg's first wait, so every later Xorg wait must drain the
queue. `X11_READY_CHIRHO` remains only as a temporary console-trace input; it is
not readiness authority.

Production proof gate: a KVM serial log must show which wait primitive Xorg
actually calls. If no Xorg epoll syscall appears, the epoll hook is unreachable
and this workflow must move its accepting transition to the primitive Xorg does
use rather than widening a PID gate.
