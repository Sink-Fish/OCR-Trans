#import <ScreenCaptureKit/ScreenCaptureKit.h>

// A key, ordered NSWindow may still belong to a disconnected display's Space.
// Ask the compositor about this one window without enumerating other apps.
bool ocr_window_is_onscreen(intptr_t number) {
    if (number <= 0) return false;
    NSArray<NSDictionary *> *rows = CFBridgingRelease(
        CGWindowListCopyWindowInfo(kCGWindowListOptionIncludingWindow, (CGWindowID)number));
    return [rows.firstObject[(__bridge NSString *)kCGWindowIsOnscreen] boolValue];
}

// A change means that the user may have deliberately switched apps. Read counts
// only; no event tap, key contents or extra input-monitoring permission is needed.
uint64_t ocr_input_presses(void) {
    const CGEventType types[] = { kCGEventKeyDown, kCGEventLeftMouseDown,
        kCGEventRightMouseDown, kCGEventOtherMouseDown };
    uint64_t count = 0;
    for (size_t i = 0; i < sizeof(types) / sizeof(types[0]); ++i)
        count += CGEventSourceCounterForEventType(kCGEventSourceStateCombinedSessionState, types[i]);
    return count;
}

// Runs only in the disposable capture worker. Its parent enforces the deadline
// across both asynchronous operations and reaps the worker on timeout.
int ocr_capture_display(uint32_t displayID, uint32_t width, uint32_t height,
                        uint8_t *rgba, char *message, size_t capacity) {
    if (@available(macOS 14.0, *)) {
        @autoreleasepool {
            dispatch_semaphore_t done = dispatch_semaphore_create(0);
            __block NSError *failure = nil;
            __block CGImageRef frame = NULL;
            [SCShareableContent getShareableContentExcludingDesktopWindows:NO
                onScreenWindowsOnly:YES completionHandler:^(SCShareableContent *content, NSError *error) {
                    for (SCDisplay *display in content.displays) {
                        if (display.displayID != displayID) continue;
                        SCContentFilter *filter = [[SCContentFilter alloc] initWithDisplay:display excludingWindows:@[]];
                        if (@available(macOS 14.2, *)) filter.includeMenuBar = YES;
                        SCStreamConfiguration *config = [SCStreamConfiguration new];
                        config.width = width;
                        config.height = height;
                        config.showsCursor = NO;
                        [SCScreenshotManager captureImageWithFilter:filter configuration:config
                            completionHandler:^(CGImageRef image, NSError *captureError) {
                                if (image) frame = CGImageRetain(image);
                                failure = captureError;
                                dispatch_semaphore_signal(done);
                            }];
                        return;
                    }
                    failure = error;
                    dispatch_semaphore_signal(done);
                }];
            dispatch_semaphore_wait(done, DISPATCH_TIME_FOREVER);
            if (!frame) {
                snprintf(message, capacity, "%s", failure ? failure.localizedDescription.UTF8String : "显示器已断开连接");
                return [failure.domain isEqualToString:SCStreamErrorDomain] && failure.code == SCStreamErrorUserDeclined ? 2 : 3;
            }

            CGColorSpaceRef space = CGColorSpaceCreateWithName(kCGColorSpaceSRGB);
            CGContextRef ctx = CGBitmapContextCreate(rgba, width, height, 8, (size_t)width * 4,
                space, kCGImageAlphaPremultipliedLast | kCGBitmapByteOrder32Big);
            CGColorSpaceRelease(space);
            if (ctx) {
                CGContextDrawImage(ctx, CGRectMake(0, 0, width, height), frame);
                CGContextRelease(ctx);
            }
            CGImageRelease(frame);
            if (!ctx) {
                snprintf(message, capacity, "无法创建截图像素缓冲区");
                return 3;
            }
            return 0;
        }
    }
    return 1; // macOS 13 uses CoreGraphics.
}
