# Linux notes

## NVIDIA graphics

Chord uses WebKitGTK to draw its window. On NVIDIA with Wayland, the terminal can show lines like this:

```
MESA-EGL: warning: pci id for fd 25: 10de:2684, driver (null)
MESA-EGL: warning: egl: failed to create dri2 screen
```

These lines come from the Mesa EGL library. It tries the NVIDIA device and fails, then a different path works. They are noise and do not stop the app.

The same setup can also close the window at start with "Error 71 (Protocol error) dispatching to Wayland display". Chord sets `__NV_DISABLE_EXPLICIT_SYNC=1` when it finds the NVIDIA driver. This keeps hardware acceleration. Chord does not change a variable that you set yourself.

## If the window still fails

Try these variables in this order. Each one costs more speed than the one before.

1. `__NV_DISABLE_EXPLICIT_SYNC=0` turns the Chord default off.
2. `WEBKIT_DISABLE_DMABUF_RENDERER=1` uses a slower render path.
3. `WEBKIT_DISABLE_COMPOSITING_MODE=1` turns off accelerated drawing. Use it as a last step.

Example: `WEBKIT_DISABLE_DMABUF_RENDERER=1 chord-desktop`

See the Tauri guide: <https://v2.tauri.app/develop/debug/linux-graphics/>
