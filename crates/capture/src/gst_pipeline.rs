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

/// The whole pipeline: source → 30 fps → output size → tee into the MP4
/// encoder and a small RGBA appsink (named `thumb`) for the thumbnail.
#[allow(clippy::too_many_arguments)]
pub fn pipeline(
    source: &str,
    fps: u32,
    out_width: u32,
    out_height: u32,
    encoder: &str,
    parser: &str,
    path: &str,
    thumb_width: u32,
    thumb_height: u32,
) -> String {
    format!(
        "{source} ! queue ! videoconvert ! videorate ! video/x-raw,framerate={fps}/1 \
         ! videoscale ! video/x-raw,width={out_width},height={out_height},pixel-aspect-ratio=1/1 ! tee name=t \
         t. ! queue ! videoconvert ! video/x-raw,format=I420 ! {encoder} ! {parser} \
         ! mp4mux faststart=true ! filesink location={} \
         t. ! queue leaky=downstream max-size-buffers=1 ! videoconvert ! videoscale \
         ! video/x-raw,format=RGBA,width={thumb_width},height={thumb_height} \
         ! appsink name=thumb max-buffers=1 drop=true sync=false wait-on-eos=false",
        quote(path)
    )
}

/// Thumbnail size: at most `max_width` wide, aspect kept, at least 2x2.
pub fn thumb_size(width: u32, height: u32, max_width: u32) -> (u32, u32) {
    if width <= max_width {
        return (width.max(2), height.max(2));
    }
    let h = (height as u64 * max_width as u64 / width as u64) as u32;
    (max_width, h.max(2))
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
            "x264enc",
            "h264parse",
            "/tmp/My Videos/rec \"1\".mp4",
            320,
            240,
        );
        assert!(p.contains("framerate=30/1"));
        assert!(p.contains("width=800,height=600"));
        assert!(
            p.contains("location=\"/tmp/My Videos/rec \\\"1\\\".mp4\""),
            "{p}"
        );
        assert!(p.contains("appsink name=thumb"));
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

    #[test]
    fn thumbnails_keep_the_aspect_ratio() {
        assert_eq!(thumb_size(1600, 1000, 640), (640, 400));
        assert_eq!(thumb_size(300, 200, 640), (300, 200));
        assert_eq!(thumb_size(5000, 2, 640), (640, 2));
    }
}
