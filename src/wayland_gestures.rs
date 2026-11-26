use std::{
    io::ErrorKind,
    os::fd::{AsRawFd, FromRawFd, OwnedFd},
    thread::{self, JoinHandle},
};

use wayland_client::{
    Connection, Dispatch, Proxy, QueueHandle,
    backend::Backend,
    protocol::{wl_pointer, wl_registry, wl_seat},
};
use wayland_protocols::wp::pointer_gestures::zv1::client::{
    zwp_pointer_gesture_pinch_v1, zwp_pointer_gesture_swipe_v1, zwp_pointer_gestures_v1,
};
use winit::{
    event_loop::EventLoopProxy,
    raw_window_handle::{HasDisplayHandle, RawDisplayHandle},
    window::Window,
};

#[derive(Clone, Copy, Debug)]
pub enum GestureEvent {
    PinchBegin,
    PinchUpdate {
        dx: f32,
        dy: f32,
        scale: f32,
        rotation: f32,
    },
    PinchEnd,
    SwipeBegin {
        fingers: u32,
    },
    SwipeUpdate {
        dx: f32,
        dy: f32,
    },
    SwipeEnd,
}

struct SeatEntry {
    name: u32,
    seat: wl_seat::WlSeat,
    has_pointer: bool,
    pointer: Option<wl_pointer::WlPointer>,
    pinch: Option<zwp_pointer_gesture_pinch_v1::ZwpPointerGesturePinchV1>,
    swipe: Option<zwp_pointer_gesture_swipe_v1::ZwpPointerGestureSwipeV1>,
}

struct WaylandState<T: 'static> {
    proxy: EventLoopProxy<T>,
    make_event: fn(GestureEvent) -> T,
    _registry: Option<wl_registry::WlRegistry>,
    gestures: Option<zwp_pointer_gestures_v1::ZwpPointerGesturesV1>,
    seats: Vec<SeatEntry>,
}

impl<T: 'static> WaylandState<T> {
    fn new(proxy: EventLoopProxy<T>, make_event: fn(GestureEvent) -> T) -> Self {
        Self {
            proxy,
            make_event,
            _registry: None,
            gestures: None,
            seats: Vec::new(),
        }
    }

    fn emit(&self, ev: GestureEvent) {
        let _ = self.proxy.send_event((self.make_event)(ev));
    }

    fn sync_seat(&mut self, name: u32, qh: &QueueHandle<Self>) {
        let Some(gestures) = &self.gestures else {
            return;
        };
        let Some(entry) = self.seats.iter_mut().find(|s| s.name == name) else {
            return;
        };
        if entry.has_pointer {
            if entry.pointer.is_none() {
                let ptr = entry.seat.get_pointer(qh, name);
                entry.pinch = Some(gestures.get_pinch_gesture(&ptr, qh, ()));
                entry.swipe = Some(gestures.get_swipe_gesture(&ptr, qh, ()));
                entry.pointer = Some(ptr);
            }
        } else {
            if let Some(pinch) = entry.pinch.take() {
                pinch.destroy();
            }
            if let Some(swipe) = entry.swipe.take() {
                swipe.destroy();
            }
            if let Some(ptr) = entry.pointer.take()
                && ptr.version() >= 3
            {
                ptr.release();
            }
        }
    }

    fn sync_all_seats(&mut self, qh: &QueueHandle<Self>) {
        let names: Vec<u32> = self.seats.iter().map(|s| s.name).collect();
        for name in names {
            self.sync_seat(name, qh);
        }
    }
}

impl<T: 'static> Dispatch<wl_registry::WlRegistry, ()> for WaylandState<T> {
    fn event(
        state: &mut Self,
        registry: &wl_registry::WlRegistry,
        event: wl_registry::Event,
        _: &(),
        _: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        match event {
            wl_registry::Event::Global {
                name,
                interface,
                version,
            } => {
                if interface == "zwp_pointer_gestures_v1" {
                    let g = registry.bind::<zwp_pointer_gestures_v1::ZwpPointerGesturesV1, _, _>(
                        name,
                        version.min(3),
                        qh,
                        (),
                    );
                    state.gestures = Some(g);
                    state.sync_all_seats(qh);
                } else if interface == "wl_seat" {
                    let seat =
                        registry.bind::<wl_seat::WlSeat, _, _>(name, version.min(7), qh, name);
                    state.seats.push(SeatEntry {
                        name,
                        seat,
                        has_pointer: false,
                        pointer: None,
                        pinch: None,
                        swipe: None,
                    });
                }
            }
            wl_registry::Event::GlobalRemove { name } => {
                if let Some(idx) = state.seats.iter().position(|s| s.name == name) {
                    let mut entry = state.seats.remove(idx);
                    if let Some(pinch) = entry.pinch.take() {
                        pinch.destroy();
                    }
                    if let Some(swipe) = entry.swipe.take() {
                        swipe.destroy();
                    }
                    if let Some(ptr) = entry.pointer.take()
                        && ptr.version() >= 3
                    {
                        ptr.release();
                    }
                    if entry.seat.version() >= 5 {
                        entry.seat.release();
                    }
                }
            }
            _ => {}
        }
    }
}

impl<T: 'static> Dispatch<wl_seat::WlSeat, u32> for WaylandState<T> {
    fn event(
        state: &mut Self,
        _: &wl_seat::WlSeat,
        event: wl_seat::Event,
        name: &u32,
        _: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        if let wl_seat::Event::Capabilities { capabilities } = event {
            let has_ptr = capabilities
                .into_result()
                .is_ok_and(|c| c.contains(wl_seat::Capability::Pointer));
            if let Some(entry) = state.seats.iter_mut().find(|s| s.name == *name) {
                entry.has_pointer = has_ptr;
            }
            state.sync_seat(*name, qh);
        }
    }
}

impl<T: 'static> Dispatch<wl_pointer::WlPointer, u32> for WaylandState<T> {
    fn event(
        _: &mut Self,
        _: &wl_pointer::WlPointer,
        _: wl_pointer::Event,
        _: &u32,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl<T: 'static> Dispatch<zwp_pointer_gestures_v1::ZwpPointerGesturesV1, ()> for WaylandState<T> {
    fn event(
        _: &mut Self,
        _: &zwp_pointer_gestures_v1::ZwpPointerGesturesV1,
        _: zwp_pointer_gestures_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl<T: 'static> Dispatch<zwp_pointer_gesture_pinch_v1::ZwpPointerGesturePinchV1, ()>
    for WaylandState<T>
{
    fn event(
        state: &mut Self,
        _: &zwp_pointer_gesture_pinch_v1::ZwpPointerGesturePinchV1,
        event: zwp_pointer_gesture_pinch_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        match event {
            zwp_pointer_gesture_pinch_v1::Event::Begin { .. } => {
                state.emit(GestureEvent::PinchBegin);
            }
            zwp_pointer_gesture_pinch_v1::Event::Update {
                dx,
                dy,
                scale,
                rotation,
                ..
            } => {
                state.emit(GestureEvent::PinchUpdate {
                    dx: dx as f32,
                    dy: dy as f32,
                    scale: scale as f32,
                    rotation: rotation as f32,
                });
            }
            zwp_pointer_gesture_pinch_v1::Event::End { .. } => {
                state.emit(GestureEvent::PinchEnd);
            }
            _ => {}
        }
    }
}

impl<T: 'static> Dispatch<zwp_pointer_gesture_swipe_v1::ZwpPointerGestureSwipeV1, ()>
    for WaylandState<T>
{
    fn event(
        state: &mut Self,
        _: &zwp_pointer_gesture_swipe_v1::ZwpPointerGestureSwipeV1,
        event: zwp_pointer_gesture_swipe_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        match event {
            zwp_pointer_gesture_swipe_v1::Event::Begin { fingers, .. } => {
                state.emit(GestureEvent::SwipeBegin { fingers });
            }
            zwp_pointer_gesture_swipe_v1::Event::Update { dx, dy, .. } => {
                state.emit(GestureEvent::SwipeUpdate {
                    dx: dx as f32,
                    dy: dy as f32,
                });
            }
            zwp_pointer_gesture_swipe_v1::Event::End { .. } => {
                state.emit(GestureEvent::SwipeEnd);
            }
            _ => {}
        }
    }
}

pub struct WaylandGestures {
    stop_write: OwnedFd,
    thread: Option<JoinHandle<()>>,
}

impl Drop for WaylandGestures {
    fn drop(&mut self) {
        let byte = [1u8; 1];
        unsafe {
            libc::write(
                self.stop_write.as_raw_fd(),
                byte.as_ptr().cast(),
                byte.len(),
            );
        }
        if let Some(handle) = self.thread.take() {
            let _ = handle.join();
        }
    }
}

fn create_stop_pipe() -> Option<(OwnedFd, OwnedFd)> {
    let mut fds = [-1i32; 2];
    if unsafe { libc::pipe2(fds.as_mut_ptr(), libc::O_CLOEXEC | libc::O_NONBLOCK) } != 0 {
        return None;
    }
    let read_fd = unsafe { OwnedFd::from_raw_fd(fds[0]) };
    let write_fd = unsafe { OwnedFd::from_raw_fd(fds[1]) };
    Some((read_fd, write_fd))
}

pub fn start<T: Send + 'static>(
    window: &Window,
    proxy: EventLoopProxy<T>,
    make_event: fn(GestureEvent) -> T,
) -> Option<WaylandGestures> {
    let RawDisplayHandle::Wayland(wd) = window.display_handle().ok()?.as_raw() else {
        return None;
    };
    let backend = unsafe { Backend::from_foreign_display(wd.display.as_ptr().cast()) };
    let conn = Connection::from_backend(backend);
    let mut queue = conn.new_event_queue::<WaylandState<T>>();
    let qh = queue.handle();
    let mut state = WaylandState::new(proxy, make_event);
    state._registry = Some(conn.display().get_registry(&qh, ()));

    queue.roundtrip(&mut state).ok()?;
    state.gestures.as_ref()?;
    queue.roundtrip(&mut state).ok()?;
    let _ = conn.flush();

    let (stop_read, stop_write) = create_stop_pipe()?;
    let thread = thread::Builder::new()
        .name("mithril-wl-gestures".into())
        .spawn(move || {
            let stop_fd = stop_read.as_raw_fd();
            loop {
                while let Ok(n) = queue.dispatch_pending(&mut state) {
                    if n == 0 {
                        break;
                    }
                }
                let _ = conn.flush();
                let Some(guard) = queue.prepare_read() else {
                    continue;
                };
                let wl_fd = guard.connection_fd().as_raw_fd();
                let mut fds = [
                    libc::pollfd {
                        fd: wl_fd,
                        events: libc::POLLIN,
                        revents: 0,
                    },
                    libc::pollfd {
                        fd: stop_fd,
                        events: libc::POLLIN,
                        revents: 0,
                    },
                ];
                let mut stopped = false;
                loop {
                    let rc = unsafe { libc::poll(fds.as_mut_ptr(), 2, -1) };
                    if rc < 0 {
                        if std::io::Error::last_os_error().kind() == ErrorKind::Interrupted {
                            continue;
                        }
                        stopped = true;
                        break;
                    }
                    break;
                }
                if stopped || (fds[1].revents & (libc::POLLIN | libc::POLLHUP | libc::POLLERR)) != 0
                {
                    drop(guard);
                    break;
                }
                if (fds[0].revents & (libc::POLLERR | libc::POLLHUP | libc::POLLNVAL)) != 0 {
                    drop(guard);
                    break;
                }
                if (fds[0].revents & libc::POLLIN) != 0 {
                    match guard.read() {
                        Ok(_) => {}
                        Err(wayland_client::backend::WaylandError::Io(e))
                            if e.kind() == ErrorKind::WouldBlock => {}
                        Err(_) => break,
                    }
                } else {
                    drop(guard);
                }
            }
            for entry in &mut state.seats {
                if let Some(pinch) = entry.pinch.take() {
                    pinch.destroy();
                }
                if let Some(swipe) = entry.swipe.take() {
                    swipe.destroy();
                }
                if let Some(ptr) = entry.pointer.take()
                    && ptr.version() >= 3
                {
                    ptr.release();
                }
                if entry.seat.version() >= 5 {
                    entry.seat.release();
                }
            }
            let _ = conn.flush();
        })
        .ok()?;

    Some(WaylandGestures {
        stop_write,
        thread: Some(thread),
    })
}
