use telorgon::application_primitives::prelude::{
    VideoColorMetadata, VideoFit, VideoProtection, VideoSurface, VideoSurfaceContent,
    VideoSurfaceError, VideoSurfaceToken,
};
use telorgon::core::SizeI;

#[test]
fn public_video_surface_retains_revisioned_fit_color_and_protection_metadata() {
    let content = VideoSurfaceContent::new(
        VideoSurfaceToken::new(84).unwrap(),
        21,
        SizeI {
            width: 3840,
            height: 2160,
        },
        VideoColorMetadata::default(),
        VideoProtection::Protected,
    )
    .unwrap();
    let surface = VideoSurface::decorative(content, VideoFit::Contain);

    assert_eq!(surface.content().surface().get(), 84);
    assert_eq!(surface.content().content_version(), 21);
    assert_eq!(surface.content().frame_size().width, 3840);
    assert_eq!(surface.content().protection(), VideoProtection::Protected);
    assert_eq!(surface.fit(), VideoFit::Contain);
    assert_eq!(
        VideoSurfaceContent::new(
            VideoSurfaceToken::new(84).unwrap(),
            22,
            SizeI {
                width: 0,
                height: 2160,
            },
            VideoColorMetadata::default(),
            VideoProtection::Unprotected,
        ),
        Err(VideoSurfaceError::InvalidFrameSize)
    );
}
