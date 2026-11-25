//! GStreamer pipeline descriptions for the Linux backend. Pure string
//! building, so it is unit tested on every platform.

/// H.264 encoders to try, best first, as (factory name, description
/// template). `{kbps}` and `{bps}` are replaced with the target bitrate.
pub const ENCODERS: &[(&str, &str)] = &[
    (
        "x264enc",
        "x264enc tune=zerolatency speed-preset=veryfast bitrate={kbps} key-int-max=60",
    ),
    ("openh264enc", "openh264enc bitrate={bps}"),
    ("vah264enc", "vah264enc bitrate={kbps}"),
    ("vaapih264enc", "vaapih264enc bitrate={kbps}"),
];

/// What goes between the encoder and `mp4mux`: `h264parse` when available
/// (gstreamer1.0-plugins-bad), else x264's own AVC output. `None` when
/// neither works.
pub fn parser(has_h264parse: bool, encoder: &str) -> Option<&'static str> {
    if has_h264parse {
        Some("h264parse")
    } else if encoder.starts_with("x264enc") {
        Some("video/x-h264,stream-format=avc,alignment=au")
    } else {
        None
    }
}

/// The description for the first available encoder.
pub fn encoder_description(available: impl Fn(&str) -> bool, bitrate: u32) -> Option<String> {
    ENCODERS
        .iter()
        .find(|(name, _)| available(name))
        .map(|(_, template)| {
            template
                .replace("{kbps}", &(bitrate / 1000).to_string())
                .replace("{bps}", &bitrate.to_string())
        })
}

/// A crop rectangle in source pixels.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PixelRect {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

/// `ximagesrc` for a region of the X11 root window (end coordinates are
/// inclusive).
pub fn x11_source(display_name: &str, r: PixelRect, show_pointer: bool) -> String {
    format!(
        "ximagesrc display-name={} startx={} starty={} endx={} endy={} use-damage=false show-pointer={}",
        quote(display_name),
        r.x,
        r.y,
        r.x + r.width - 1,
        r.y + r.height - 1,
        show_pointer
    )
}

/// `pipewiresrc` for a portal stream, scaled to the monitor's size in pixels
/// and cropped to the region.
pub fn pipewire_source(
    fd: i32,
    node: u32,
    monitor_width: u32,
    monitor_height: u32,
    r: PixelRect,
) -> String {
    let right = monitor_width.saturating_sub(r.x + r.width);
    let bottom = monitor_height.saturating_sub(r.y + r.height);
    format!(
        "pipewiresrc fd={fd} path={node} do-timestamp=true keepalive-time=1000 always-copy=true \
         ! videoconvert ! videoscale ! video/x-raw,width={monitor_width},height={monitor_height} \
         ! videocrop left={} top={} right={right} bottom={bottom}",
        r.x, r.y
    )
}

/// The whole pipeline: source → 30 fps → output size → an optional overlay
/// (named `mask`, see [`exclude_pixels`]) → tee into the MP4 encoder and a
/// full-size BGRA appsink (named `thumb`, behind a valve named `firstframe`)
/// that hands over the first frame.
#[allow(clippy::too_many_arguments)]
pub fn pipeline(
    source: &str,
    fps: u32,
    out_width: u32,
    out_height: u32,
    mask: bool,
    encoder: &str,
    parser: &str,
    path: &str,
) -> String {
    let mask = if mask {
        "! gdkpixbufoverlay name=mask "
    } else {
        ""
    };
    format!(
        "{source} ! queue ! videoconvert ! videorate ! video/x-raw,framerate={fps}/1 \
         ! videoscale ! video/x-raw,width={out_width},height={out_height},pixel-aspect-ratio=1/1 \
         {mask}! tee name=t \
         t. ! queue ! videoconvert ! video/x-raw,format=I420 ! {encoder} ! {parser} \
         ! mp4mux faststart=true ! filesink location={} \
         t. ! queue leaky=downstream max-size-buffers=1 ! valve name=firstframe ! videoconvert \
         ! video/x-raw,format=BGRA ! appsink name=thumb max-buffers=1 drop=true sync=false wait-on-eos=false",
        quote(path)
    )
}

/// The area to cover, in output pixels: `exclude` is relative to a region
/// of `region_width` x `region_height` logical units, scaled by `scale` to
/// capture pixels and then to the `out_width` x `out_height` video. Grown
/// by a few pixels for anti-aliasing and clamped to the frame.
pub fn exclude_pixels(
    exclude: (f64, f64, f64, f64),
    scale: f64,
    capture: (u32, u32),
    out: (u32, u32),
) -> Option<PixelRect> {
    const MARGIN: f64 = 3.0;
    let (ex, ey, ew, eh) = exclude;
    let sx = scale * out.0 as f64 / capture.0.max(1) as f64;
    let sy = scale * out.1 as f64 / capture.1.max(1) as f64;
    let x0 = (ex * sx - MARGIN).floor().max(0.0);
    let y0 = (ey * sy - MARGIN).floor().max(0.0);
    let x1 = ((ex + ew) * sx + MARGIN).ceil().min(out.0 as f64);
    let y1 = ((ey + eh) * sy + MARGIN).ceil().min(out.1 as f64);
    (x1 - x0 >= 1.0 && y1 - y0 >= 1.0).then_some(PixelRect {
        x: x0 as u32,
        y: y0 as u32,
        width: (x1 - x0) as u32,
        height: (y1 - y0) as u32,
    })
}

/// Quote a value for `gst_parse_launch`.
pub fn quote(s: &str) -> String {
    format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\""))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn picks_the_first_available_encoder() {
        let only_openh264 = |n: &str| n == "openh264enc";
        assert_eq!(
            encoder_description(only_openh264, 6_000_000).unwrap(),
            "openh264enc bitrate=6000000"
        );
        let all = |_: &str| true;
        assert!(
            encoder_description(all, 6_000_000)
                .unwrap()
                .starts_with("x264enc")
        );
        assert!(
            encoder_description(all, 6_000_000)
                .unwrap()
                .contains("bitrate=6000")
        );
        assert!(encoder_description(|_| false, 1).is_none());
    }

    #[test]
    fn x11_source_uses_inclusive_end_coordinates() {
        let s = x11_source(
            ":0",
            PixelRect {
                x: 10,
                y: 20,
                width: 400,
                height: 300,
            },
            true,
        );
        assert!(s.contains("startx=10 starty=20 endx=409 endy=319"), "{s}");
        assert!(s.contains("display-name=\":0\""));
        assert!(s.contains("show-pointer=true"));
    }

    #[test]
    fn pipewire_source_crops_from_every_side() {
        let s = pipewire_source(
            7,
            42,
            1920,
            1080,
            PixelRect {
                x: 100,
                y: 50,
                width: 800,
                height: 600,
            },
        );
        assert!(s.contains("fd=7 path=42"));
        assert!(s.contains("width=1920,height=1080"));
        assert!(
            s.contains("videocrop left=100 top=50 right=1020 bottom=430"),
            "{s}"
        );
    }

    #[test]
    fn pipeline_writes_the_quoted_path() {
        let p = pipeline(
            "videotestsrc",
            30,
            800,
            600,
            true,
            "x264enc",
            "h264parse",
            "/tmp/My Videos/rec \"1\".mp4",
        );
        assert!(p.contains("framerate=30/1"));
        assert!(p.contains("width=800,height=600"));
        assert!(
            p.contains("location=\"/tmp/My Videos/rec \\\"1\\\".mp4\""),
            "{p}"
        );
        assert!(p.contains("appsink name=thumb"));
        assert!(p.contains("gdkpixbufoverlay name=mask"));
        assert!(p.contains("valve name=firstframe"));
        let plain = pipeline(
            "videotestsrc",
            30,
            800,
            600,
            false,
            "x264enc",
            "h264parse",
            "/tmp/a.mp4",
        );
        assert!(!plain.contains("gdkpixbufoverlay"));
    }

    #[test]
    fn maps_the_pill_into_video_pixels() {
        // A 168x40 pill at (1736, 1024) in a full-HD region, 1:1.
        let r = exclude_pixels(
            (1736.0, 1024.0, 168.0, 40.0),
            1.0,
            (1920, 1080),
            (1920, 1080),
        )
        .unwrap();
        assert_eq!((r.x, r.y), (1733, 1021));
        assert_eq!((r.width, r.height), (174, 46));
        // HiDPI (scale 2) and a video scaled down to half the capture size.
        let r =
            exclude_pixels((100.0, 50.0, 168.0, 40.0), 2.0, (3840, 2160), (1920, 1080)).unwrap();
        assert_eq!((r.x, r.y, r.width, r.height), (97, 47, 174, 46));
        // Entirely outside the frame: nothing to cover.
        assert!(
            exclude_pixels((5000.0, 5000.0, 10.0, 10.0), 1.0, (100, 100), (100, 100)).is_none()
        );
    }

    #[test]
    fn falls_back_to_x264_avc_output_without_h264parse() {
        assert_eq!(parser(true, "openh264enc"), Some("h264parse"));
        assert_eq!(
            parser(false, "x264enc tune=zerolatency"),
            Some("video/x-h264,stream-format=avc,alignment=au")
        );
        assert_eq!(parser(false, "openh264enc"), None);
    }
}
