# Vivid Virtual Camera Troubleshooting

Quick reference for debugging vivid test patterns and capture issues.

## Finding Vivid Devices

Vivid devices may not be at `/dev/video0`. Find them by name:

```bash
# List all devices
v4l2-ctl --list-devices

# Find vivid specifically
for dev in /sys/class/video4linux/video*; do
    name=$(cat "$dev/name" 2>/dev/null)
    echo "$(basename $dev): $name"
done | grep -i vivid
```

## Check Current Configuration

```bash
# Format and controls
v4l2-ctl -d /dev/video2 --get-fmt-video
v4l2-ctl -d /dev/video2 -l | grep test_pattern
```

## Available Test Patterns

List patterns on your system (may vary by kernel version):

```bash
v4l2-ctl -d /dev/video2 --list-ctrls-menus | grep -A 30 test_pattern
```

| # | Pattern | Description |
|---|---------|-------------|
| 0 | 75% Colorbar | SMPTE color bars at 75% intensity |
...
| 20 | Gray Ramp | Horizontal luminance gradient |
| 21 | Noise | Random noise pattern |

## Set Test Pattern

```bash
v4l2-ctl -d /dev/video2 --set-ctrl=test_pattern=20
```

## Set Format

```bash
# YUYV 640x480 (recommended for testing)
v4l2-ctl -d /dev/video2 --set-fmt-video=width=640,height=480,pixelformat=YUYV

# Verify
v4l2-ctl -d /dev/video2 --get-fmt-video
```

## View Live Stream

```bash
ffplay -f v4l2 /dev/video2
```

If that fails with buffer errors, specify format explicitly:

```bash
ffplay -f v4l2 -input_format yuyv422 -video_size 640x480 /dev/video2
```

## Capture and View Frames

```bash
# Capture single frame
v4l2-ctl -d /dev/video2 --stream-mmap --stream-count=1 --stream-to=/tmp/frame.raw

# View with ffplay (match format to capture)
ffplay -f rawvideo -pixel_format yuyv422 -video_size 640x480 /tmp/frame.raw
```

## Common Issues

### Garbage format values (e.g., 12682x8640)

Vivid state is corrupted. Reload the module:

```bash
sudo modprobe -r vivid
sudo modprobe vivid n_devs=2 node_types=0x1,0x1 input_types=0x81,0x81
```

### "Module vivid is in use"

Something is holding the device open:

```bash
# Find processes
sudo lsof /dev/video2 /dev/video3 2>/dev/null
sudo fuser -v /dev/video2 /dev/video3

# Kill them
sudo fuser -k /dev/video2 /dev/video3
```

### "ioctl(VIDIOC_QBUF): Bad file descriptor"

Buffer handling issue with ffplay. Use v4l2-ctl instead:

```bash
v4l2-ctl -d /dev/video2 --stream-mmap --stream-count=1 --stream-to=/tmp/frame.raw
ffplay -f rawvideo -pixel_format yuyv422 -video_size 640x480 /tmp/frame.raw
```

### Wrong pixel format in ffplay

Check actual format and match it:

```bash
v4l2-ctl -d /dev/video2 --get-fmt-video
# Look at "Pixel Format" field:
# - YUYV -> -pixel_format yuyv422
# - YU12 -> -pixel_format yuv420p
```

## Reload Vivid with Configuration

```bash
./scripts/dev-setup.sh unload-vivid
./scripts/dev-setup.sh load-vivid
```

This sets:
- Device 1: Gray Ramp (pattern 20) - gradient for `validate_gradient()`
- Device 2: 100% Colorbar (pattern 1) - SMPTE bars for `validate_color_bars()`