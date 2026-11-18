#import "GBReaderViewController.h"
#import "GBReaderParts.h"
#import "GMAppDelegate.h"
#import "GMArt.h"
#import "GMCompat.h"
#import "GMCore.h"
#import "GMLog.h"
#import "GMTheme.h"
#import <QuartzCore/QuartzCore.h>

/// Chờ WebKit vẽ xong vùng vừa cuộn tới rồi mới chụp (iPad mini 1 — chỉnh khi thử).
static const NSTimeInterval kPaint = 0.10;

static id GBParse(NSString *s) {
    if (!s.length) return nil;
    return [NSJSONSerialization JSONObjectWithData:[s dataUsingEncoding:NSUTF8StringEncoding] options:0 error:NULL];
}

static NSString *GBJSON(id obj) {
    if (!obj) return @"null";
    NSData *d = [NSJSONSerialization dataWithJSONObject:@[obj] options:0 error:NULL];
    NSString *s = d ? [[NSString alloc] initWithData:d encoding:NSUTF8StringEncoding] : @"[null]";
    return [s substringWithRange:NSMakeRange(1, s.length - 2)];  // bọc trong mảng để mã được cả chuỗi / số
}

static BOOL GBPadIdiom(void) {
    return UI_USER_INTERFACE_IDIOM() == UIUserInterfaceIdiomPad;
}

@interface GBReaderViewController () <UIWebViewDelegate, UIPageViewControllerDataSource, UIPageViewControllerDelegate,
                                      UIGestureRecognizerDelegate, UIPopoverControllerDelegate, UIAlertViewDelegate>
@end

@implementation GBReaderViewController {
    NSDictionary *_info;          // sách lúc mở (thư viện)
    NSDictionary *_book;          // kết quả book.open
    NSString *_bookId, *_base;
    NSArray *_spine, *_toc;
    NSArray *_marks;
    NSDictionary *_cfg;
    UIColor *_paper, *_paperBack;  // nền trang · mặt sau tờ đang lật cong
    // Khung
    UIPageViewController *_pager;
    UIPanGestureRecognizer *_pagerPan;
    UIWebView *_web;
    UIView *_touch;
    UIImageView *_cover, *_slideA, *_slideB;
    UILabel *_header, *_footer;
    UILabel *_clock, *_battery;     // giờ (trái), pin (phải) — dòng đầu trang
    UIImageView *_battIcon;
    UIView *_dimView;
    UIView *_warmView;              // ánh sáng ấm: lớp hổ phách (GBReaderPrefs warm)
    UIBarButtonItem *_nightItem;    // iPad: trăng / mặt trời — bật / tắt giao diện tối
    UIPanGestureRecognizer *_lightPan;  // vuốt dọc ở mép trái: độ sáng
    UILabel *_lightHUD;
    CGFloat _lightStart;
    NSTimer *_tickTimer;
    CFTimeInterval _loadStart;
    UIView *_loading;
    UILabel *_loadLabel;
    UIActivityIndicatorView *_spin;
    UIToolbar *_top;
    UIView *_bottom;
    UISlider *_slider;
    UILabel *_bottomLabel, *_titleLabel;
    UIBarButtonItem *_tocItem, *_aaItem, *_markItem;
    UIButton *_backBtn;
    UIPopoverController *_popover;
    // Trạng thái
    NSInteger _s, _p, _parts, _page, _screens;
    BOOL _opened, _loadingChapter, _awaitReady, _busy, _snapping, _chrome, _closing;
    id _target;                    // @"start" | @"end" | pos dict | id (chuỗi) | tỉ lệ (NSNumber)
    NSDictionary *_transition;     // {from: ảnh, dir}
    NSInteger _pendingTurn, _snapGen;
    UIImage *_imgCur, *_imgNext, *_imgPrev;
    NSMutableArray *_back;
    NSTimer *_saveTimer, *_idleTimer;
    NSDate *_lastTurn;
    double _secs;
    CGSize _laidOut;
    NSString *_externalURL;
    NSString *_anchorC;            // chỗ đang đọc lúc lật / nhảy gần nhất — giữ qua các lần dàn lại (cỡ chữ, xoay máy)
}

- (instancetype)initWithBook:(NSDictionary *)book {
    if ((self = [super init])) {
        _info = book;
        _key = GMVal(book, @"key");
        _back = [NSMutableArray array];
        self.wantsFullScreenLayout = YES;
        _pendingTurn = 0;
    }
    return self;
}

- (void)dealloc {
    [[NSNotificationCenter defaultCenter] removeObserver:self];
    [_saveTimer invalidate];
    [_idleTimer invalidate];
    [_tickTimer invalidate];
    [self dropPopover];
    _pager.dataSource = nil;  // iOS 5 / 6: hai cái này là assign — lật dở mà màn đọc đóng thì UIKit gọi vào vùng nhớ đã giải phóng
    _pager.delegate = nil;
    _web.delegate = nil;
    [UIDevice currentDevice].batteryMonitoringEnabled = NO;
}

/// UIPopoverController bị giải phóng khi còn hiện → UIKit ném lỗi «dealloc reached while popover is still visible» (iPad).
- (void)dropPopover {
    _popover.delegate = nil;
    [_popover dismissPopoverAnimated:NO];
    _popover = nil;
}

#pragma mark - Dựng khung

- (void)viewDidLoad {
    [super viewDidLoad];
    _paper = [GBReaderPrefs paperFor:[GBReaderPrefs theme]];
    _paperBack = [GBReaderPrefs backFor:[GBReaderPrefs theme]];
    self.view.backgroundColor = _paper;
    CGRect b = self.view.bounds;
    // Lớp dưới: lật cong bằng ảnh chụp trang. Hai mặt: mặt sau do app vẽ (giấy ngà) — một mặt thì iOS phủ trắng lên chữ in ngược.
    _pager = [[UIPageViewController alloc] initWithTransitionStyle:UIPageViewControllerTransitionStylePageCurl
                                             navigationOrientation:UIPageViewControllerNavigationOrientationHorizontal
                                                           options:@{UIPageViewControllerOptionSpineLocationKey: @(UIPageViewControllerSpineLocationMin)}];
    _pager.dataSource = self;
    _pager.delegate = self;
    _pager.doubleSided = YES;
    [self addChildViewController:_pager];
    _pager.view.frame = b;
    _pager.view.autoresizingMask = UIViewAutoresizingFlexibleWidth | UIViewAutoresizingFlexibleHeight;
    [self.view addSubview:_pager.view];
    [_pager didMoveToParentViewController:self];
    [self showLeaf:nil page:0];
    // WebView của chương: trong suốt — chỗ chưa vẽ kịp lộ đúng ảnh trang bên dưới
    _web = [[UIWebView alloc] initWithFrame:b];
    _web.autoresizingMask = UIViewAutoresizingFlexibleWidth | UIViewAutoresizingFlexibleHeight;
    _web.delegate = self;
    _web.scalesPageToFit = NO;
    _web.opaque = NO;
    _web.backgroundColor = [UIColor clearColor];
    _web.dataDetectorTypes = UIDataDetectorTypeNone;
    _web.scrollView.scrollEnabled = NO;
    _web.scrollView.bounces = NO;
    _web.scrollView.showsHorizontalScrollIndicator = NO;
    _web.scrollView.showsVerticalScrollIndicator = NO;
    if ([_web respondsToSelector:@selector(setSuppressesIncrementalRendering:)]) {
        GM_IOS6_BEGIN
        _web.suppressesIncrementalRendering = YES;
        GM_IOS6_END
    }
    [self.view addSubview:_web];
    // Ảnh phủ (đổi chương / chụp trang kế) và hai ảnh cho kiểu lật trượt
    _cover = [[UIImageView alloc] initWithFrame:b];
    _cover.autoresizingMask = UIViewAutoresizingFlexibleWidth | UIViewAutoresizingFlexibleHeight;
    _cover.hidden = YES;
    [self.view addSubview:_cover];
    _slideA = [[UIImageView alloc] initWithFrame:b];
    _slideB = [[UIImageView alloc] initWithFrame:b];
    _slideA.hidden = _slideB.hidden = YES;
    [self.view addSubview:_slideB];
    [self.view addSubview:_slideA];
    // Dòng đầu trang (tên chương) / chân trang (số trang)
    _header = [self smallLabel];
    _footer = [self smallLabel];
    [self.view addSubview:_header];
    [self.view addSubview:_footer];
    _clock = [self smallLabel];
    _clock.textAlignment = GMAlignLeft;
    _battery = [self smallLabel];
    _battery.textAlignment = GMAlignRight;
    _battIcon = [[UIImageView alloc] initWithFrame:CGRectMake(0, 0, 22, 10)];
    [self.view addSubview:_clock];
    [self.view addSubview:_battery];
    [self.view addSubview:_battIcon];
    [UIDevice currentDevice].batteryMonitoringEnabled = YES;
    [[NSNotificationCenter defaultCenter] addObserver:self selector:@selector(updateBattery)
                                                 name:UIDeviceBatteryLevelDidChangeNotification object:nil];
    [[NSNotificationCenter defaultCenter] addObserver:self selector:@selector(updateBattery)
                                                 name:UIDeviceBatteryStateDidChangeNotification object:nil];
    // Lớp nhận chạm: chạm (của app) + vuốt cong (của UIPageViewController)
    _touch = [[UIView alloc] initWithFrame:b];
    _touch.autoresizingMask = UIViewAutoresizingFlexibleWidth | UIViewAutoresizingFlexibleHeight;
    _touch.backgroundColor = [UIColor clearColor];
    [self.view addSubview:_touch];
    for (UIGestureRecognizer *g in _pager.gestureRecognizers) {
        if ([g isKindOfClass:[UIPanGestureRecognizer class]]) {
            _pagerPan = (UIPanGestureRecognizer *)g;
            [_touch addGestureRecognizer:g];
        } else {
            g.enabled = NO;  // chạm của app lo việc lật bằng chạm
        }
    }
    UITapGestureRecognizer *tap = [[UITapGestureRecognizer alloc] initWithTarget:self action:@selector(tapped:)];
    [_touch addGestureRecognizer:tap];
    _lightPan = [[UIPanGestureRecognizer alloc] initWithTarget:self action:@selector(lightPanned:)];
    _lightPan.delegate = self;
    [_touch addGestureRecognizer:_lightPan];
    [_pagerPan requireGestureRecognizerToFail:_lightPan];  // vuốt dọc ở mép trái không lật trang
    // Nút «Quay lại» sau khi nhảy (mục lục, link)
    _backBtn = [UIButton buttonWithType:UIButtonTypeCustom];
    _backBtn.titleLabel.font = [UIFont boldSystemFontOfSize:13];
    _backBtn.backgroundColor = [UIColor colorWithWhite:0 alpha:0.6];
    _backBtn.layer.cornerRadius = 14;
    [_backBtn setTitleColor:[UIColor whiteColor] forState:UIControlStateNormal];
    [_backBtn addTarget:self action:@selector(goBack) forControlEvents:UIControlEventTouchUpInside];
    _backBtn.hidden = YES;
    [self.view addSubview:_backBtn];
    [self buildChrome];
    _dimView = [[UIView alloc] initWithFrame:b];
    _dimView.autoresizingMask = UIViewAutoresizingFlexibleWidth | UIViewAutoresizingFlexibleHeight;
    _dimView.backgroundColor = [UIColor blackColor];
    _dimView.userInteractionEnabled = NO;
    _dimView.alpha = [GBReaderPrefs dim];
    [self.view addSubview:_dimView];
    _warmView = [[UIView alloc] initWithFrame:b];
    _warmView.autoresizingMask = UIViewAutoresizingFlexibleWidth | UIViewAutoresizingFlexibleHeight;
    _warmView.backgroundColor = [UIColor colorWithRed:1 green:0.56 blue:0.12 alpha:1];  // ~2700 K
    _warmView.userInteractionEnabled = NO;
    [self.view addSubview:_warmView];
    [self applyLight];
    _lightHUD = [[UILabel alloc] initWithFrame:CGRectMake(0, 0, 150, 36)];
    _lightHUD.backgroundColor = [UIColor colorWithWhite:0 alpha:0.7];
    _lightHUD.textColor = [UIColor whiteColor];
    _lightHUD.font = [UIFont boldSystemFontOfSize:14];
    _lightHUD.textAlignment = GMAlignCenter;
    _lightHUD.layer.cornerRadius = 8;
    _lightHUD.clipsToBounds = YES;
    _lightHUD.alpha = 0;
    _lightHUD.userInteractionEnabled = NO;
    [self.view addSubview:_lightHUD];
    [self buildLoading];
    [[NSNotificationCenter defaultCenter] addObserver:self selector:@selector(coreEvent:) name:GMCoreEventNotification object:nil];
    [[NSNotificationCenter defaultCenter] addObserver:self selector:@selector(willResign) name:UIApplicationWillResignActiveNotification object:nil];
    [self open];
}

- (UILabel *)smallLabel {
    UILabel *l = [[UILabel alloc] init];
    l.font = [UIFont systemFontOfSize:GBPadIdiom() ? 12 : 11];
    l.textAlignment = GMAlignCenter;
    l.backgroundColor = [UIColor clearColor];
    l.textColor = [[GBReaderPrefs inkFor:[GBReaderPrefs theme]] colorWithAlphaComponent:0.45];
    l.lineBreakMode = GMTailTruncation;
    l.userInteractionEnabled = NO;
    return l;
}

- (void)buildLoading {
    _loading = [[UIView alloc] initWithFrame:self.view.bounds];
    _loading.autoresizingMask = UIViewAutoresizingFlexibleWidth | UIViewAutoresizingFlexibleHeight;
    _loading.backgroundColor = _paper;
    _spin = [[UIActivityIndicatorView alloc] initWithActivityIndicatorStyle:[[GBReaderPrefs theme] isEqual:@"day"] || [[GBReaderPrefs theme] isEqual:@"sepia"]
                                                                                ? UIActivityIndicatorViewStyleGray
                                                                                : UIActivityIndicatorViewStyleWhite];
    _spin.center = CGPointMake(_loading.bounds.size.width / 2, _loading.bounds.size.height / 2 - 20);
    _spin.autoresizingMask = UIViewAutoresizingFlexibleLeftMargin | UIViewAutoresizingFlexibleRightMargin |
                             UIViewAutoresizingFlexibleTopMargin | UIViewAutoresizingFlexibleBottomMargin;
    [_spin startAnimating];
    [_loading addSubview:_spin];
    _loadLabel = [[UILabel alloc] initWithFrame:CGRectMake(20, _spin.center.y + 24, _loading.bounds.size.width - 40, 60)];
    _loadLabel.autoresizingMask = UIViewAutoresizingFlexibleWidth | UIViewAutoresizingFlexibleTopMargin | UIViewAutoresizingFlexibleBottomMargin;
    _loadLabel.numberOfLines = 3;
    _loadLabel.textAlignment = GMAlignCenter;
    _loadLabel.backgroundColor = [UIColor clearColor];
    _loadLabel.textColor = [GBReaderPrefs inkFor:[GBReaderPrefs theme]];
    _loadLabel.font = [UIFont systemFontOfSize:15];
    _loadLabel.text = [NSString stringWithFormat:@"%@\nĐang mở sách…", GMVal(_info, @"title") ?: @""];
    [_loading addSubview:_loadLabel];
    UIButton *x = [UIButton buttonWithType:UIButtonTypeCustom];
    x.frame = CGRectMake(8, 8, 44, 44);
    [x setImage:[GMArt glyph:@"close" size:22 color:[GBReaderPrefs inkFor:[GBReaderPrefs theme]]] forState:UIControlStateNormal];
    [x addTarget:self action:@selector(close) forControlEvents:UIControlEventTouchUpInside];
    [_loading addSubview:x];
    [self.view addSubview:_loading];
}

- (void)buildChrome {
    CGRect b = self.view.bounds;
    _top = [[UIToolbar alloc] initWithFrame:CGRectMake(0, 0, b.size.width, 44)];
    _top.barStyle = UIBarStyleBlack;
    _top.translucent = YES;
    _top.autoresizingMask = UIViewAutoresizingFlexibleWidth;
    UIBarButtonItem *lib = [[UIBarButtonItem alloc] initWithTitle:@"Thư viện" style:UIBarButtonItemStyleBordered target:self action:@selector(close)];
    _tocItem = [[UIBarButtonItem alloc] initWithTitle:@"Mục lục" style:UIBarButtonItemStyleBordered target:self action:@selector(showToc)];
    _aaItem = [[UIBarButtonItem alloc] initWithTitle:@"Aa" style:UIBarButtonItemStyleBordered target:self action:@selector(showStyle)];
    _markItem = [[UIBarButtonItem alloc] initWithImage:[GMArt glyph:@"bookmark" size:20 color:[UIColor whiteColor]] style:UIBarButtonItemStylePlain
                                                target:self action:@selector(toggleMark)];
    _titleLabel = [[UILabel alloc] initWithFrame:CGRectMake(0, 0, MAX(60, b.size.width - (GBPadIdiom() ? 380 : 330)), 30)];
    _titleLabel.backgroundColor = [UIColor clearColor];
    _titleLabel.textColor = [UIColor whiteColor];
    _titleLabel.font = [UIFont boldSystemFontOfSize:15];
    _titleLabel.textAlignment = GMAlignCenter;
    _titleLabel.lineBreakMode = GMTailTruncation;
    _titleLabel.text = GMVal(_info, @"title");
    _titleLabel.autoresizingMask = UIViewAutoresizingFlexibleWidth;
    UIBarButtonItem *flex = [[UIBarButtonItem alloc] initWithBarButtonSystemItem:UIBarButtonSystemItemFlexibleSpace target:nil action:nil];
    UIBarButtonItem *flex2 = [[UIBarButtonItem alloc] initWithBarButtonSystemItem:UIBarButtonSystemItemFlexibleSpace target:nil action:nil];
    if (GBPadIdiom()) {
        _nightItem = [[UIBarButtonItem alloc] initWithImage:[self nightGlyph] style:UIBarButtonItemStylePlain target:self
                                                     action:@selector(toggleNight)];
        _top.items = @[lib, _tocItem, flex, [[UIBarButtonItem alloc] initWithCustomView:_titleLabel], flex2, _nightItem, _aaItem, _markItem];
    } else {
        _top.items = @[lib, _tocItem, flex, [[UIBarButtonItem alloc] initWithCustomView:_titleLabel], flex2, _aaItem, _markItem];
    }
    _top.alpha = 0;
    [self.view addSubview:_top];
    _bottom = [[UIView alloc] initWithFrame:CGRectMake(0, b.size.height - 64, b.size.width, 64)];
    _bottom.autoresizingMask = UIViewAutoresizingFlexibleWidth | UIViewAutoresizingFlexibleTopMargin;
    _bottom.backgroundColor = [UIColor colorWithWhite:0.08 alpha:0.86];
    _slider = [[UISlider alloc] initWithFrame:CGRectMake(20, 6, b.size.width - 40, 30)];
    _slider.autoresizingMask = UIViewAutoresizingFlexibleWidth;
    _slider.minimumValue = 0;
    [_slider addTarget:self action:@selector(sliderMoved) forControlEvents:UIControlEventValueChanged];
    [_slider addTarget:self action:@selector(sliderDone) forControlEvents:UIControlEventTouchUpInside | UIControlEventTouchUpOutside];
    [_bottom addSubview:_slider];
    _bottomLabel = [[UILabel alloc] initWithFrame:CGRectMake(10, 38, b.size.width - 20, 20)];
    _bottomLabel.autoresizingMask = UIViewAutoresizingFlexibleWidth;
    _bottomLabel.backgroundColor = [UIColor clearColor];
    _bottomLabel.textColor = [UIColor colorWithWhite:0.85 alpha:1];
    _bottomLabel.font = [UIFont systemFontOfSize:13];
    _bottomLabel.textAlignment = GMAlignCenter;
    [_bottom addSubview:_bottomLabel];
    _bottom.alpha = 0;
    [self.view addSubview:_bottom];
}

- (void)viewWillAppear:(BOOL)animated {
    [super viewWillAppear:animated];
    [[UIApplication sharedApplication] setStatusBarHidden:YES withAnimation:UIStatusBarAnimationFade];
}

- (void)viewDidAppear:(BOOL)animated {
    [super viewDidAppear:animated];
    [self keepAwake];
    [self tick];
    [self updateBattery];
    [_tickTimer invalidate];
    _tickTimer = [NSTimer scheduledTimerWithTimeInterval:15 target:self selector:@selector(tick) userInfo:nil repeats:YES];
}

- (void)viewDidDisappear:(BOOL)animated {
    [super viewDidDisappear:animated];
    // NSTimer giữ self: phải huỷ thì màn đọc mới giải phóng được (đóng bằng đường nào cũng vậy); hiện lại thì viewDidAppear đặt lại
    [_tickTimer invalidate];
    _tickTimer = nil;
    [_idleTimer invalidate];
    _idleTimer = nil;
}

- (void)viewWillDisappear:(BOOL)animated {
    [super viewWillDisappear:animated];
    if (self.isBeingDismissed) [self shutdown];  // bị đóng từ ngoài, không qua nút «Thư viện»
    [self dropPopover];  // popover chỉ có trên iPad, nơi màn đọc chỉ biến mất khi bị đóng — popover không được ở lại trên cửa sổ
    [[UIApplication sharedApplication] setStatusBarHidden:NO withAnimation:UIStatusBarAnimationFade];
    [UIApplication sharedApplication].idleTimerDisabled = NO;
}

#pragma mark - Xoay máy

- (BOOL)shouldAutorotateToInterfaceOrientation:(UIInterfaceOrientation)o {
    return GBPadIdiom() || o != UIInterfaceOrientationPortraitUpsideDown;
}

- (BOOL)shouldAutorotate {
    return YES;
}

- (UIInterfaceOrientationMask)supportedInterfaceOrientations {
    return (UIInterfaceOrientationMask)(GBPadIdiom() ? 30 : 26);  // (mọi hướng) / (trừ úp ngược) — số của UIInterfaceOrientationMask (iOS 6)
}

- (void)viewWillLayoutSubviews {
    [super viewWillLayoutSubviews];
    CGRect b = self.view.bounds;
    CGFloat top = GBPadIdiom() ? 16 : 8, side = GBPadIdiom() ? 24 : 10, corner = GBPadIdiom() ? 110 : 70;
    _header.frame = CGRectMake(corner, top, b.size.width - 2 * corner, 16);
    _footer.frame = CGRectMake(40, b.size.height - (GBPadIdiom() ? 30 : 22), b.size.width - 80, 16);
    _clock.frame = CGRectMake(side, top, corner - side, 16);
    _battIcon.frame = CGRectMake(b.size.width - side - 22, top + 3, 22, 10);
    _battery.frame = CGRectMake(b.size.width - side - 22 - 4 - 50, top, 50, 16);
    _lightHUD.center = CGPointMake(GBPadIdiom() ? 120 : 90, b.size.height / 2);
    _backBtn.frame = CGRectMake(12, b.size.height - (GBPadIdiom() ? 76 : 64), 150, 28);
    if (_opened && !CGSizeEqualToSize(_laidOut, b.size)) {
        _laidOut = b.size;
        [self applyConfigKeepingPosition:YES];
    }
}

#pragma mark - Mở sách

- (void)open {
    [GMCore call:@"book.open" args:@{@"key": _key ?: @""} done:^(id ok, NSDictionary *err) {
        if (_closing) return;
        if (err) {
            GMLog(@"[đọc] mở sách lỗi %@", GMVal(err, @"code"));
            [[[UIAlertView alloc] initWithTitle:@"Không mở được sách" message:GMErrorText(err) delegate:self cancelButtonTitle:@"Đóng"
                              otherButtonTitles:nil] show];
            return;
        }
        _book = ok;
        _bookId = GMVal(ok, @"id");
        [GBReaderViewController rememberOpen:_info];
        _base = GMVal(ok, @"base");
        _spine = GMVal(ok, @"spine") ?: @[];
        _toc = GMVal(ok, @"toc") ?: @[];
        NSDictionary *reading = GMVal(ok, @"reading");
        _marks = GMVal(reading, @"marks") ?: @[];
        _titleLabel.text = GMVal(ok, @"title");
        _opened = YES;
        _laidOut = self.view.bounds.size;
        [self sendConfig];
        NSDictionary *pos = GMVal(reading, @"pos");
        if ([pos isKindOfClass:[NSDictionary class]] && [GMVal(pos, @"s") integerValue] < (NSInteger)_spine.count) {
            [self loadSpine:[GMVal(pos, @"s") integerValue] part:[GMVal(pos, @"p") integerValue] target:pos];
        } else {
            [self loadSpine:[self firstLinear] part:0 target:@"start"];
        }
    }];
}

- (void)alertView:(UIAlertView *)a clickedButtonAtIndex:(NSInteger)i {
    if (_externalURL) {
        if (i == 1) [[UIApplication sharedApplication] openURL:[NSURL URLWithString:_externalURL]];
        _externalURL = nil;
        return;
    }
    [self close];
}

- (NSInteger)firstLinear {
    for (NSUInteger i = 0; i < _spine.count; i++) {
        if ([GMVal([_spine objectAtIndex:i], @"linear") boolValue]) return i;
    }
    return 0;
}

- (void)coreEvent:(NSNotification *)n {
    NSString *e = GMVal(n.userInfo, @"e");
    if (_opened || ![GMVal(n.userInfo, @"key") isEqual:_key]) return;
    if ([e isEqual:@"file.progress"]) {
        double have = [GMVal(n.userInfo, @"have") doubleValue], total = [GMVal(n.userInfo, @"total") doubleValue];
        if (total > 0) _loadLabel.text = [NSString stringWithFormat:@"%@\nĐang tải sách… %.0f%%", GMVal(_info, @"title") ?: @"", have * 100 / total];
    }
}

#pragma mark - Cấu hình màn đọc

/// Khung + kiểu chữ hiện tại (reader.js dựng CSS từ đây).
- (NSDictionary *)config {
    CGSize sz = self.view.bounds.size;
    BOOL pad = GBPadIdiom();
    BOOL land = sz.width > sz.height;
    NSArray *font = [[GBReaderPrefs fonts] objectAtIndex:[GBReaderPrefs fontIndex]];
    double fs = [[[GBReaderPrefs sizes] objectAtIndex:[GBReaderPrefs sizeIndex]] doubleValue] * [[font objectAtIndex:2] doubleValue];
    NSInteger mi = [GBReaderPrefs marginIndex];
    // (cú pháp @[…][i] cần iOS 6 — dùng objectAtIndex:)
    double ms = pad ? [[@[@26, @44, @72] objectAtIndex:mi] doubleValue] : [[@[@12, @20, @32] objectAtIndex:mi] doubleValue];
    if (pad && land && [GBReaderPrefs twoPages]) ms = [[@[@24, @36, @56] objectAtIndex:mi] doubleValue];
    double lh = [[@[@1.3, @1.5, @1.75] objectAtIndex:[GBReaderPrefs lineIndex]] doubleValue];
    NSString *theme = [GBReaderPrefs theme];
    return @{
        @"w": @(sz.width), @"h": @(sz.height), @"cols": @((pad && land && [GBReaderPrefs twoPages]) ? 2 : 1),
        @"ms": @(ms), @"mt": @(pad ? 48 : 30), @"mb": @(pad ? 44 : 28),
        @"fs": @(round(fs * 2) / 2), @"lh": @(lh), @"ff": [font objectAtIndex:1], @"theme": theme,
        @"just": @([GBReaderPrefs justify]), @"weight": @([GBReaderPrefs weight]), @"st": [GBReaderPrefs publisher] ? @"pub" : @"gb",
        @"nf": @([GMVal(_book, @"nf") boolValue] || [GMVal(_info, @"nf") boolValue]),  // «hạn chế lưu nội dung»: không chọn chữ
    };
}

- (void)sendConfig {
    _cfg = [self config];
    [GMCore call:@"reader.config" args:@{@"config": _cfg} done:nil];
}

/// Đổi cấu hình (Aa, xoay máy): áp ngay cho chương đang mở (giữ chỗ đang đọc), lưu cho các chương sau.
- (void)applyConfigKeepingPosition:(BOOL)keep {
    NSString *theme = [GBReaderPrefs theme];
    _paper = [GBReaderPrefs paperFor:theme];
    _paperBack = [GBReaderPrefs backFor:theme];
    self.view.backgroundColor = _paper;
    _loading.backgroundColor = _paper;
    UIColor *ink = [[GBReaderPrefs inkFor:theme] colorWithAlphaComponent:0.45];
    _header.textColor = _footer.textColor = _clock.textColor = _battery.textColor = ink;
    [self updateBattery];
    [self applyLight];
    [self sendConfig];
    _imgCur = _imgNext = _imgPrev = nil;
    if (!_opened || _loadingChapter) return;
    NSString *info = [self js:[NSString stringWithFormat:@"GB.setConfig(%@, %@)", GBJSON(_cfg), _anchorC ? GBJSON(_anchorC) : @"null"]];
    NSDictionary *d = GBParse(info);
    if (d) {
        _screens = MAX(1, [GMVal(d, @"screens") integerValue]);
        _page = MIN([GMVal(d, @"page") integerValue], _screens - 1);
    }
    [self scrollToPage:_page];
    [self updateLabels];
    [self snapSoon];
    [self saveSoon];
}

#pragma mark - Nạp chương

- (NSString *)js:(NSString *)code {
    return [_web stringByEvaluatingJavaScriptFromString:code];
}

- (void)loadSpine:(NSInteger)s part:(NSInteger)p target:(id)target {
    if (s < 0 || s >= (NSInteger)_spine.count) return;
    _loadingChapter = YES;
    _awaitReady = NO;
    _s = s;
    _p = MAX(0, p);
    _target = target;
    _imgCur = _imgNext = _imgPrev = nil;
    _snapGen++;
    _loadStart = CACurrentMediaTime();
    NSString *href = GMVal([_spine objectAtIndex:s], @"href") ?: @"";
    NSString *url = [NSString stringWithFormat:@"%@%@?p=%ld", _base, href, (long)_p];
    [_web loadRequest:[NSURLRequest requestWithURL:[NSURL URLWithString:url]]];
}

- (BOOL)webView:(UIWebView *)w shouldStartLoadWithRequest:(NSURLRequest *)r navigationType:(UIWebViewNavigationType)t {
    NSURL *u = r.URL;
    if ([u.scheme isEqual:@"gb"]) {
        NSArray *msgs = GBParse([self js:@"GB.drain()"]);
        for (NSDictionary *m in msgs) {
            if ([GMVal(m, @"e") isEqual:@"ready"] && _awaitReady) {
                _awaitReady = NO;
                [self chapterReady:m];
            }
        }
        return NO;
    }
    if (t == UIWebViewNavigationTypeLinkClicked) {
        [self followLink:@{@"href": u.absoluteString ?: @""}];
        return NO;
    }
    return YES;
}

- (void)webViewDidFinishLoad:(UIWebView *)w {
    if (w.isLoading || !_loadingChapter) return;
    NSDictionary *info = GBParse([self js:@"window.GB ? GB.info() : ''"]);
    if (!info) {
        GMLog(@"[đọc] chương %ld không có reader.js", (long)_s);
        [self chapterReady:@{@"screens": @1, @"n": @1}];
        return;
    }
    if (![GMVal(info, @"ready") boolValue]) {
        _awaitReady = YES;  // chờ tin «ready» (ảnh chưa về)
        return;
    }
    [self chapterReady:info];
}

- (void)webView:(UIWebView *)w didFailLoadWithError:(NSError *)error {
    if (error.code == NSURLErrorCancelled || error.code == 102 /* WebKitErrorFrameLoadInterruptedByPolicyChange */) return;
    GMLog(@"[đọc] nạp chương lỗi %@", error.localizedDescription);
    if (_loadingChapter) {
        // Cổng có thể đã bị iOS thu hồi lúc nằm nền (M23) → mở lại rồi thử lại một lần
        _loadingChapter = NO;
        [GMCore call:@"serve.revive" args:@{} done:^(id ok, NSDictionary *err) {
            [self loadSpine:_s part:_p target:_target];
        }];
    }
}

- (void)chapterReady:(NSDictionary *)info {
    _loadingChapter = NO;
    [[NSUserDefaults standardUserDefaults] setInteger:0 forKey:@"gb.restoreTries"];  // mở lại được → lần sau vẫn mở lại
    _parts = MAX(1, [GMVal(info, @"n") integerValue]);
    _screens = MAX(1, [GMVal(info, @"screens") integerValue]);
    GMLog(@"[đọc] chương %ld phần %ld/%ld: %ld màn, %.0f ms", (long)_s, (long)_p + 1, (long)_parts, (long)_screens,
          (CACurrentMediaTime() - _loadStart) * 1000);
    NSInteger page = 0;
    id t = _target;
    if ([t isEqual:@"end"]) {
        page = [[self js:@"GB.goToEnd()"] integerValue];
    } else if ([t isKindOfClass:[NSDictionary class]]) {
        NSString *c = GMVal(t, @"c");
        page = c.length ? [[self js:[NSString stringWithFormat:@"GB.goToPos(%@)", GBJSON(c)]] integerValue] : -1;
        if (page < 0) page = [[self js:[NSString stringWithFormat:@"GB.goToFraction(%f)", [GMVal(t, @"f") doubleValue]]] integerValue];
    } else if ([t isKindOfClass:[NSString class]] && ![t isEqual:@"start"] && [t length]) {
        page = [[self js:[NSString stringWithFormat:@"GB.goToId(%@)", GBJSON(t)]] integerValue];
    } else if ([t isKindOfClass:[NSNumber class]]) {
        page = [[self js:[NSString stringWithFormat:@"GB.goToFraction(%f)", [t doubleValue]]] integerValue];
    }
    _page = MAX(0, MIN(page, _screens - 1));
    _target = nil;
    [self scrollToPage:_page];
    if (_loading.superview) {
        [UIView animateWithDuration:0.25 animations:^{ _loading.alpha = 0; } completion:^(BOOL f) { [_loading removeFromSuperview]; }];
    }
    NSDictionary *tr = _transition;
    _transition = nil;
    if (tr) {
        // Đổi chương: chờ vẽ, chụp trang mới, rồi lật từ ảnh trang cũ (đang phủ) sang
        dispatch_after(dispatch_time(DISPATCH_TIME_NOW, (int64_t)(kPaint * 1.5 * NSEC_PER_SEC)), dispatch_get_main_queue(), ^{
            UIImage *to = [self renderWeb];
            [self animateFrom:GMVal(tr, @"from") to:to dir:[GMVal(tr, @"dir") integerValue] page:_page];
        });
        return;
    }
    _cover.hidden = YES;
    _web.hidden = NO;
    [self settled];
}

#pragma mark - Đi trang

- (void)scrollToPage:(NSInteger)p {
    [self js:[NSString stringWithFormat:@"GB.goTo(%ld)", (long)p]];
    CGFloat w = [GMVal(_cfg, @"w") doubleValue] ?: self.view.bounds.size.width;
    _web.scrollView.contentOffset = CGPointMake(p * w, 0);
}

- (UIImage *)renderWeb {
    CGSize sz = _web.bounds.size;
    UIGraphicsBeginImageContextWithOptions(sz, YES, 0);
    CGContextRef ctx = UIGraphicsGetCurrentContext();
    [_paper setFill];
    CGContextFillRect(ctx, CGRectMake(0, 0, sz.width, sz.height));
    [_web.layer renderInContext:ctx];
    UIImage *img = UIGraphicsGetImageFromCurrentImageContext();
    UIGraphicsEndImageContext();
    return img;
}

- (void)coverWith:(UIImage *)img {
    _cover.image = img;
    _cover.frame = self.view.bounds;
    _cover.hidden = NO;
}

- (GBPageVC *)frontOf:(UIImage *)img page:(NSInteger)page {
    return [GBPageVC pageWithImage:img page:page paper:_paper];
}

- (GBPageVC *)backOf:(UIImage *)img page:(NSInteger)page {
    return [GBPageVC backWithImage:img page:page paper:_paperBack];
}

/// Đặt trang đứng yên cho lớp lật cong. Không hiệu ứng thì chỉ một trang — đưa kèm mặt sau là UIKit ném lỗi (đã thử iOS 6.1.3);
/// mặt sau chỉ đưa khi lật có hiệu ứng (animateFrom).
- (void)showLeaf:(UIImage *)img page:(NSInteger)page {
    [_pager setViewControllers:@[[self frontOf:img page:page]]
                     direction:UIPageViewControllerNavigationDirectionForward animated:NO completion:nil];
}

/// Trang trước / sau. Trong chương: dùng ảnh đã chụp sẵn (không có thì chụp ngay); hết chương: nạp chương kế rồi lật.
- (void)turn:(NSInteger)dir {
    if (!_opened || _loadingChapter || _busy) return;
    if (_snapping) {
        _pendingTurn = dir;
        return;
    }
    [self hideChrome];
    NSInteger target = _page + dir;
    if (target >= 0 && target < _screens) {
        _busy = YES;
        UIImage *from = _imgCur ?: [self renderWeb];
        UIImage *to = dir > 0 ? _imgNext : _imgPrev;
        if (to) {
            [self animateFrom:from to:to dir:dir page:target];
            return;
        }
        [self coverWith:from];
        [self scrollToPage:target];
        dispatch_after(dispatch_time(DISPATCH_TIME_NOW, (int64_t)(kPaint * NSEC_PER_SEC)), dispatch_get_main_queue(), ^{
            [self animateFrom:from to:[self renderWeb] dir:dir page:target];
        });
        return;
    }
    [self changeChapter:dir];
}

/// Sang phần / chương kế (dir > 0) hoặc trước (dir < 0).
- (void)changeChapter:(NSInteger)dir {
    NSInteger s = _s, p = _p + dir;
    BOOL samePart = p >= 0 && p < _parts;
    if (!samePart) {
        s = _s + dir;
        while (s >= 0 && s < (NSInteger)_spine.count && ![GMVal([_spine objectAtIndex:s], @"linear") boolValue]) s += dir;
        if (s < 0 || s >= (NSInteger)_spine.count) {
            [GMTheme toast:dir > 0 ? @"Hết sách" : @"Đầu sách" in:self.view];
            if (dir > 0) [GMCore call:@"reading.finish" args:@{@"key": _key, @"on": @YES} done:nil];
            return;
        }
    }
    _busy = YES;
    UIImage *from = _imgCur ?: [self renderWeb];
    [self coverWith:from];
    _transition = @{@"from": from, @"dir": @(dir)};
    if (samePart) {
        [self loadSpine:s part:p target:dir > 0 ? @"start" : @"end"];
        return;
    }
    if (dir > 0) {
        [self loadSpine:s part:0 target:@"start"];
        return;
    }
    // Lùi về cuối chương trước: cần số phần của chương đó
    [GMCore call:@"book.chapter" args:@{@"id": _bookId ?: @"", @"s": @(s)} done:^(id ok, NSDictionary *err) {
        NSInteger parts = MAX(1, [GMVal(ok, @"parts") integerValue]);
        [self loadSpine:s part:parts - 1 target:@"end"];
    }];
}

/// Hiệu ứng lật từ ảnh `from` sang ảnh `to` theo kiểu đã chọn, xong thì đứng ở trang `page`.
- (void)animateFrom:(UIImage *)from to:(UIImage *)to dir:(NSInteger)dir page:(NSInteger)page {
    NSString *turn = [GBReaderPrefs turn];
    _busy = YES;
    [self setInfoHidden:YES];
    if ([turn isEqual:@"curl"] && from && to) {
        [self showLeaf:from page:_page];
        _web.hidden = YES;
        _cover.hidden = YES;
        __weak GBReaderViewController *ws = self;
        // Trang sẽ hiện + mặt sau của tờ cong qua: tới thì tờ đang đọc lật đi, lùi thì tờ trước lật về
        GBPageVC *back = dir > 0 ? [self backOf:from page:_page] : [self backOf:to page:page];
        [_pager setViewControllers:@[[self frontOf:to page:page], back]
                         direction:dir > 0 ? UIPageViewControllerNavigationDirectionForward : UIPageViewControllerNavigationDirectionReverse
                          animated:YES
                        completion:^(BOOL finished) { [ws settleAt:page]; }];
        return;
    }
    if ([turn isEqual:@"slide"] && from && to) {
        CGRect b = self.view.bounds;
        _slideA.image = from;
        _slideB.image = to;
        _slideA.frame = b;
        _slideB.frame = CGRectOffset(b, dir > 0 ? b.size.width : -b.size.width, 0);
        _slideA.hidden = _slideB.hidden = NO;
        _web.hidden = YES;
        _cover.hidden = YES;
        [UIView animateWithDuration:0.28 delay:0 options:UIViewAnimationOptionCurveEaseOut animations:^{
            _slideA.frame = CGRectOffset(b, dir > 0 ? -b.size.width : b.size.width, 0);
            _slideB.frame = b;
        } completion:^(BOOL f) {
            [self settleAt:page];
        }];
        return;
    }
    [self settleAt:page];
}

/// Đứng ở trang `page` (WebView đang ẩn / bị phủ): dời WebView tới đó rồi hiện lại.
- (void)settleAt:(NSInteger)page {
    _page = MAX(0, MIN(page, _screens - 1));
    [self scrollToPage:_page];
    dispatch_after(dispatch_time(DISPATCH_TIME_NOW, (int64_t)(0.04 * NSEC_PER_SEC)), dispatch_get_main_queue(), ^{
        _web.hidden = NO;
        _cover.hidden = YES;
        _slideA.hidden = _slideB.hidden = YES;
        [self settled];
    });
}

/// Trang vừa đứng yên: nhãn, lưu vị trí, chụp sẵn trang kế / trước.
- (void)settled {
    _busy = NO;
    _anchorC = GMVal([self currentPos], @"c");
    [self setInfoHidden:NO];
    [self updateLabels];
    [self noteActivity];
    [self saveSoon];
    [self snapSoon];
}

- (void)snapSoon {
    NSInteger gen = ++_snapGen;
    dispatch_after(dispatch_time(DISPATCH_TIME_NOW, (int64_t)(0.12 * NSEC_PER_SEC)), dispatch_get_main_queue(), ^{
        if (gen == _snapGen) [self snapNeighbors];
    });
}

/// Chụp trang hiện tại (đặt làm trang của UIPageViewController) và trang kế / trước: phủ ảnh trang hiện tại lên WebView, dời WebView
/// sang trang kế, chờ vẽ, chụp, rồi về — người đọc không thấy gì đổi.
- (void)snapNeighbors {
    if (!_opened || _loadingChapter || _busy || _snapping || _closing) return;
    if ([[GBReaderPrefs turn] isEqual:@"none"]) return;
    NSInteger gen = _snapGen;
    _imgCur = [self renderWeb];
    [self showLeaf:_imgCur page:_page];
    _imgNext = _imgPrev = nil;
    BOOL hasNext = _page + 1 < _screens, hasPrev = _page > 0;
    if (!hasNext && !hasPrev) return;
    _snapping = YES;
    [self coverWith:_imgCur];
    NSInteger here = _page;
    void (^finish)(void) = ^{
        [self scrollToPage:here];
        dispatch_after(dispatch_time(DISPATCH_TIME_NOW, (int64_t)(0.04 * NSEC_PER_SEC)), dispatch_get_main_queue(), ^{
            _cover.hidden = YES;
            _snapping = NO;
            NSInteger pend = _pendingTurn;
            _pendingTurn = 0;
            if (pend) [self turn:pend];
        });
    };
    void (^prev)(void) = ^{
        if (!hasPrev || gen != _snapGen) {
            finish();
            return;
        }
        [self scrollToPage:here - 1];
        dispatch_after(dispatch_time(DISPATCH_TIME_NOW, (int64_t)(kPaint * NSEC_PER_SEC)), dispatch_get_main_queue(), ^{
            if (gen == _snapGen) _imgPrev = [self renderWeb];
            finish();
        });
    };
    if (hasNext) {
        [self scrollToPage:here + 1];
        dispatch_after(dispatch_time(DISPATCH_TIME_NOW, (int64_t)(kPaint * NSEC_PER_SEC)), dispatch_get_main_queue(), ^{
            if (gen == _snapGen) _imgNext = [self renderWeb];
            prev();
        });
    } else {
        prev();
    }
}

#pragma mark - Lật cong theo ngón tay (UIPageViewController)

- (BOOL)panning {
    UIGestureRecognizerState st = _pagerPan.state;
    return st == UIGestureRecognizerStateBegan || st == UIGestureRecognizerStateChanged;
}

/// Lật hai mặt: UIPageViewController hỏi lần lượt mặt sau của tờ sẽ cong rồi tới trang kế. Tới: trang n → mặt sau tờ n →
/// trang n + 1; lùi: trang n → mặt sau tờ n − 1 → trang n − 1.
- (UIViewController *)neighbor:(NSInteger)dir of:(UIViewController *)vc {
    if (!_opened || _loadingChapter || _busy || ![[GBReaderPrefs turn] isEqual:@"curl"]) return nil;
    NSInteger target = _page + dir;
    if (_snapping || target < 0 || target >= _screens) {
        // Đang chụp / hết chương: lật bằng lệnh (cong tự động) ngay sau cử chỉ
        if ([self panning]) {
            dispatch_async(dispatch_get_main_queue(), ^{ [self turn:dir]; });
        }
        return nil;
    }
    UIImage *img = dir > 0 ? _imgNext : _imgPrev;
    if (!img) {
        if ([self panning]) dispatch_async(dispatch_get_main_queue(), ^{ [self turn:dir]; });
        return nil;
    }
    // Bắt đầu cong: ẩn WebView (trang ảnh hiện tại bên dưới giống hệt)
    [self hideChrome];
    _web.hidden = YES;
    [self setInfoHidden:YES];
    GBPageVC *pv = [vc isKindOfClass:[GBPageVC class]] ? (GBPageVC *)vc : nil;
    if (pv.back) return [self frontOf:img page:target];
    return dir > 0 ? [self backOf:pv.image ?: _imgCur page:_page] : [self backOf:img page:target];
}

- (UIViewController *)pageViewController:(UIPageViewController *)pvc viewControllerAfterViewController:(UIViewController *)vc {
    return [self neighbor:1 of:vc];
}

- (UIViewController *)pageViewController:(UIPageViewController *)pvc viewControllerBeforeViewController:(UIViewController *)vc {
    return [self neighbor:-1 of:vc];
}

- (void)pageViewController:(UIPageViewController *)pvc didFinishAnimating:(BOOL)finished
   previousViewControllers:(NSArray *)prev transitionCompleted:(BOOL)completed {
    GBPageVC *cur = nil;  // lật hai mặt: viewControllers có cả mặt sau — lấy trang đang hiện
    for (GBPageVC *v in pvc.viewControllers) {
        if ([v isKindOfClass:[GBPageVC class]] && !v.back) {
            cur = v;
            break;
        }
    }
    if (completed && cur) {
        _busy = YES;
        [self settleAt:cur.page];
    } else {
        _web.hidden = NO;
        [self setInfoHidden:NO];
    }
}

#pragma mark - Chạm

- (void)tapped:(UITapGestureRecognizer *)g {
    if (!_opened || _busy) return;
    if (_chrome) {
        [self hideChrome];
        return;
    }
    CGPoint pt = [g locationInView:_web];
    NSDictionary *hit = GBParse([self js:[NSString stringWithFormat:@"GB.tap(%.0f,%.0f)", pt.x, pt.y]]);
    NSString *k = GMVal(hit, @"k");
    if ([k isEqual:@"link"]) {
        [self followLink:hit];
        return;
    }
    if ([k isEqual:@"note"]) {
        [self showNote:GMVal(hit, @"html")];
        return;
    }
    if ([k isEqual:@"sel"]) return;
    CGFloat w = self.view.bounds.size.width;
    if (pt.x < w * 0.28) [self turn:-1];
    else if (pt.x > w * 0.72) [self turn:1];
    else [self toggleChrome];
}

- (NSDictionary *)currentPos {
    NSDictionary *pos = GBParse([self js:@"GB.pos()"]);
    NSMutableDictionary *d = [NSMutableDictionary dictionaryWithDictionary:@{@"s": @(_s), @"p": @(_p)}];
    if (GMVal(pos, @"c")) [d setObject:GMVal(pos, @"c") forKey:@"c"];
    [d setObject:@([GMVal(pos, @"f") doubleValue]) forKey:@"f"];
    if (GMVal(pos, @"t")) [d setObject:GMVal(pos, @"t") forKey:@"t"];
    return d;
}

- (void)pushBack {
    [_back addObject:[self currentPos]];
    [_backBtn setTitle:[NSString stringWithFormat:@"‹ Quay lại trang %ld", (long)_page + 1] forState:UIControlStateNormal];
    _backBtn.hidden = NO;
}

- (void)goBack {
    NSDictionary *pos = [_back lastObject];
    if (!pos) return;
    [_back removeLastObject];
    _backBtn.hidden = _back.count == 0;
    [self jumpTo:pos];
}

/// Nhảy tới vị trí (mục lục, dấu trang, link, quay lại): đổi chương nếu cần, mờ dần.
- (void)jumpTo:(NSDictionary *)pos {
    NSInteger s = [GMVal(pos, @"s") integerValue], p = [GMVal(pos, @"p") integerValue];
    id target = GMVal(pos, @"frag");
    if (![target length]) target = GMVal(pos, @"c") ? pos : @"start";
    [self hideChrome];
    if (s == _s && p == _p && !_loadingChapter) {
        NSInteger page = 0;
        if ([target isKindOfClass:[NSDictionary class]]) page = [[self js:[NSString stringWithFormat:@"GB.goToPos(%@)", GBJSON(GMVal(pos, @"c"))]] integerValue];
        else if ([target isKindOfClass:[NSString class]] && ![target isEqual:@"start"]) page = [[self js:[NSString stringWithFormat:@"GB.goToId(%@)", GBJSON(target)]] integerValue];
        [self fadeToPage:MAX(0, page)];
        return;
    }
    UIImage *from = [self renderWeb];
    [self coverWith:from];
    _transition = @{@"from": from, @"dir": @0};
    _busy = YES;
    [self loadSpine:s part:p target:target];
}

- (void)fadeToPage:(NSInteger)page {
    UIImage *from = [self renderWeb];
    [self coverWith:from];
    [self scrollToPage:page];
    _page = page;
    dispatch_after(dispatch_time(DISPATCH_TIME_NOW, (int64_t)(kPaint * NSEC_PER_SEC)), dispatch_get_main_queue(), ^{
        [UIView animateWithDuration:0.2 animations:^{ _cover.alpha = 0; } completion:^(BOOL f) {
            _cover.hidden = YES;
            _cover.alpha = 1;
            [self settled];
        }];
    });
}

- (void)followLink:(NSDictionary *)hit {
    NSString *href = GMVal(hit, @"href") ?: @"";
    NSString *local = GMVal(hit, @"local");
    if (local.length) {
        NSInteger page = [[self js:[NSString stringWithFormat:@"GB.goToId(%@)", GBJSON(local)]] integerValue];
        if (page >= 0) {
            [self scrollToPage:_page];  // goToId đã dời — về lại để hiệu ứng mờ dần đúng
            [self pushBack];
            [self fadeToPage:page];
        }
        return;
    }
    if ([href hasPrefix:@"http://127.0.0.1"]) {
        [GMCore call:@"book.locate" args:@{@"id": _bookId ?: @"", @"href": href} done:^(id ok, NSDictionary *err) {
            if (err) {
                [GMTheme toast:GMErrorText(err) in:self.view];
                return;
            }
            [self pushBack];
            [self jumpTo:@{@"s": GMVal(ok, @"s") ?: @0, @"p": GMVal(ok, @"p") ?: @0, @"frag": GMVal(ok, @"frag") ?: @""}];
        }];
        return;
    }
    if ([href hasPrefix:@"http"] || [href hasPrefix:@"mailto:"]) {
        _externalURL = href;
        [[[UIAlertView alloc] initWithTitle:@"Mở liên kết ngoài sách?" message:href delegate:self cancelButtonTitle:@"Huỷ"
                          otherButtonTitles:@"Mở Safari", nil] show];
    }
}

/// Bóng chú thích (chú thích cuối trang / chú thích kiểu Word của vnthuquan).
- (void)showNote:(NSString *)html {
    NSString *text = html ?: @"";
    NSRange r;
    while ((r = [text rangeOfString:@"<[^>]+>" options:NSRegularExpressionSearch]).location != NSNotFound) {
        text = [text stringByReplacingCharactersInRange:r withString:@" "];
    }
    text = [[text stringByReplacingOccurrencesOfString:@"&nbsp;" withString:@" "] stringByReplacingOccurrencesOfString:@"&#160;" withString:@" "];
    text = [[text componentsSeparatedByCharactersInSet:[NSCharacterSet whitespaceAndNewlineCharacterSet]] componentsJoinedByString:@" "];
    while ([text rangeOfString:@"  "].location != NSNotFound) text = [text stringByReplacingOccurrencesOfString:@"  " withString:@" "];
    text = [text stringByTrimmingCharactersInSet:[NSCharacterSet whitespaceCharacterSet]];
    if (!text.length) return;
    CGRect b = self.view.bounds;
    CGFloat w = MIN(b.size.width - 40, 520);
    UIView *box = [[UIView alloc] init];
    box.backgroundColor = [UIColor colorWithWhite:0.1 alpha:0.92];
    box.layer.cornerRadius = 10;
    UITextView *tv = [[UITextView alloc] initWithFrame:CGRectMake(10, 10, w - 20, 10)];
    tv.backgroundColor = [UIColor clearColor];
    tv.textColor = [UIColor whiteColor];
    tv.font = [UIFont systemFontOfSize:16];
    tv.editable = NO;
    tv.text = text;
    CGFloat h = MIN(b.size.height * 0.5, [text sizeWithFont:tv.font constrainedToSize:CGSizeMake(w - 36, 2000)].height + 24);
    tv.frame = CGRectMake(10, 10, w - 20, h);
    box.frame = CGRectMake((b.size.width - w) / 2, b.size.height - h - 90, w, h + 20);
    [box addSubview:tv];
    UIButton *shield = [UIButton buttonWithType:UIButtonTypeCustom];
    shield.frame = b;
    shield.backgroundColor = [UIColor colorWithWhite:0 alpha:0.15];
    [shield addTarget:shield action:@selector(removeFromSuperview) forControlEvents:UIControlEventTouchUpInside];
    [shield addSubview:box];
    [self.view addSubview:shield];
}

#pragma mark - Giờ, pin, độ sáng

/// Lớp tối thêm (dưới mức sáng thấp nhất của máy) + ánh sáng ấm (hổ phách, tối đa 30 %).
- (void)applyLight {
    _dimView.alpha = [GBReaderPrefs dim];
    _warmView.alpha = [GBReaderPrefs warm] * 0.30;
}

/// Nút trên thanh: đang sáng → trăng (bấm để tối), đang tối → mặt trời.
- (UIImage *)nightGlyph {
    return [GMArt glyph:[GMTheme dark] ? @"sun" : @"moon" size:20 color:[UIColor whiteColor]];
}

/// Bật / tắt giao diện tối của cả app ngay trên thanh màn đọc (như công tắc trong Aa).
- (void)toggleNight {
    [self dropPopover];
    [GMTheme setMode:[GMTheme dark] ? GMThemeLight : GMThemeDark];
    [(GMAppDelegate *)[UIApplication sharedApplication].delegate reloadTheme];
    [GMTheme toast:[GMTheme dark] ? @"Giao diện tối" : @"Giao diện sáng" in:self.view];
}

/// Dòng đầu trang / chân trang + giờ + pin: ẩn khi đang lật (ảnh chụp trang không có chúng).
- (void)setInfoHidden:(BOOL)h {
    _header.hidden = _footer.hidden = _clock.hidden = _battery.hidden = _battIcon.hidden = h;
}

- (void)tick {
    _clock.text = [NSDateFormatter localizedStringFromDate:[NSDate date] dateStyle:NSDateFormatterNoStyle timeStyle:NSDateFormatterShortStyle];
    // Giao diện «Tự động» tới giờ đổi (19:00 / 6:00) khi đang đọc liền: đổi nền trang ngay, khung app đổi khi đóng sách
    if (!_closing && [GMTheme wantsDark] != [GMTheme dark]) {
        [(GMAppDelegate *)[UIApplication sharedApplication].delegate reloadTheme];
    }
}

/// Pin: «82%» + hình cục pin vẽ tay cùng màu chữ (iOS 5 / 6 làm tròn 5%; −1 = máy không báo).
- (void)updateBattery {
    UIDevice *d = [UIDevice currentDevice];
    float lv = d.batteryLevel;
    BOOL known = lv >= 0;
    _battery.text = known ? [NSString stringWithFormat:@"%.0f%%", lv * 100] : @"";
    if (!known) {
        _battIcon.image = nil;
        return;
    }
    BOOL charging = d.batteryState == UIDeviceBatteryStateCharging || d.batteryState == UIDeviceBatteryStateFull;
    UIColor *ink = _battery.textColor ?: [UIColor grayColor];
    UIGraphicsBeginImageContextWithOptions(CGSizeMake(22, 10), NO, 0);
    CGContextRef c = UIGraphicsGetCurrentContext();
    [ink setStroke];
    [ink setFill];
    CGContextSetLineWidth(c, 1);
    CGContextStrokeRect(c, CGRectMake(0.5, 0.5, 18, 9));
    CGContextFillRect(c, CGRectMake(19, 3, 2, 4));
    CGContextFillRect(c, CGRectMake(2, 2, floor(15 * MIN(1, lv)), 6));
    if (charging) {
        // tia sét nhỏ (khoét trong suốt)
        CGContextSetBlendMode(c, kCGBlendModeClear);
        CGContextMoveToPoint(c, 10.5, 1);
        CGContextAddLineToPoint(c, 6.5, 5.5);
        CGContextAddLineToPoint(c, 9.5, 5.5);
        CGContextAddLineToPoint(c, 8, 9);
        CGContextAddLineToPoint(c, 12.5, 4);
        CGContextAddLineToPoint(c, 9.5, 4);
        CGContextClosePath(c);
        CGContextFillPath(c);
    }
    _battIcon.image = UIGraphicsGetImageFromCurrentImageContext();
    UIGraphicsEndImageContext();
}

- (BOOL)gestureRecognizerShouldBegin:(UIGestureRecognizer *)g {
    if (g != _lightPan) return YES;
    if (!_opened || _chrome) return NO;
    CGPoint t = [_lightPan translationInView:_touch];
    CGFloat x0 = [_lightPan locationInView:_touch].x - t.x;  // chỗ đặt ngón tay
    return x0 < (GBPadIdiom() ? 44 : 30) && fabs(t.y) > fabs(t.x) * 1.5;
}

/// Vuốt dọc ở mép trái: lên sáng hơn, xuống tối hơn (dưới mức thấp nhất của máy thì thêm lớp tối) — như Moon+ / Kobo.
- (void)lightPanned:(UIPanGestureRecognizer *)g {
    if (g.state == UIGestureRecognizerStateBegan) {
        _lightStart = [GBReaderPrefs light];
        [self.view bringSubviewToFront:_lightHUD];
        [UIView animateWithDuration:0.15 animations:^{ _lightHUD.alpha = 1; }];
    }
    if (g.state == UIGestureRecognizerStateBegan || g.state == UIGestureRecognizerStateChanged) {
        CGFloat dy = [g translationInView:_touch].y;
        CGFloat v = MAX(-0.6, MIN(1, _lightStart - dy / (self.view.bounds.size.height * 0.6) * 1.6));
        [GBReaderPrefs setLight:v];
        [self applyLight];
        _lightHUD.text = v >= 0 ? [NSString stringWithFormat:@"Độ sáng %.0f%%", v * 100]
                                : [NSString stringWithFormat:@"Tối thêm %.0f%%", -v * 100];
        return;
    }
    [UIView animateWithDuration:0.4 delay:0.5 options:0 animations:^{ _lightHUD.alpha = 0; } completion:nil];
}

#pragma mark - Thanh công cụ

- (void)toggleChrome {
    if (_chrome) [self hideChrome];
    else [self showChrome];
}

- (void)showChrome {
    if (_chrome) return;
    _chrome = YES;
    [self updateLabels];
    [self.view bringSubviewToFront:_top];
    [self.view bringSubviewToFront:_bottom];
    [self.view bringSubviewToFront:_dimView];
    [self.view bringSubviewToFront:_warmView];
    [UIView animateWithDuration:0.2 animations:^{
        _top.alpha = 1;
        _bottom.alpha = 1;
    }];
}

- (void)hideChrome {
    if (!_chrome) return;
    _chrome = NO;
    [UIView animateWithDuration:0.2 animations:^{
        _top.alpha = 0;
        _bottom.alpha = 0;
    }];
}

/// Tên chương đang đọc (mục lục: mục cuối có chương ≤ chương hiện tại).
- (NSString *)chapterTitle {
    NSString *t = nil;
    for (NSDictionary *e in _toc) {
        id s = GMVal(e, @"s");
        if (s && [s integerValue] <= _s) t = GMVal(e, @"title");
    }
    return t ?: @"";
}

/// Tiến độ cả sách ước theo cỡ chương (chưa đếm trang cả sách — GĐ2).
- (double)progress {
    double total = 0, before = 0, cur = 0;
    for (NSUInteger i = 0; i < _spine.count; i++) {
        NSDictionary *sp = [_spine objectAtIndex:i];
        if (![GMVal(sp, @"linear") boolValue]) continue;
        double sz = MAX(1, [GMVal(sp, @"size") doubleValue]);
        total += sz;
        if ((NSInteger)i < _s) before += sz;
        else if ((NSInteger)i == _s) cur = sz;
    }
    if (total <= 0) return 0;
    double within = (_p + (_page + 1.0) / MAX(1, _screens)) / MAX(1, _parts);
    return MIN(1, (before + cur * within) / total);
}

- (void)updateLabels {
    NSString *ch = [self chapterTitle];
    _header.text = ch.length ? ch : GMVal(_book, @"title");
    NSInteger left = _screens - _page - 1;
    if (_parts > 1) {
        _footer.text = [NSString stringWithFormat:@"%ld / %ld · phần %ld/%ld", (long)_page + 1, (long)_screens, (long)_p + 1, (long)_parts];
    } else {
        _footer.text = left > 0 ? [NSString stringWithFormat:@"%ld / %ld · còn %ld trang hết chương", (long)_page + 1, (long)_screens, (long)left]
                                : [NSString stringWithFormat:@"%ld / %ld", (long)_page + 1, (long)_screens];
    }
    _slider.maximumValue = MAX(1, _screens - 1);
    _slider.value = _page;
    _slider.enabled = _screens > 1;
    _bottomLabel.text = [NSString stringWithFormat:@"%@ · trang %ld/%ld · %.0f%% cả sách", ch.length ? ch : @"", (long)_page + 1, (long)_screens,
                                                   [self progress] * 100];
    BOOL marked = [self markOnPage] != nil;
    _markItem.image = [GMArt glyph:marked ? @"bookmarkFill" : @"bookmark" size:20 color:marked ? [UIColor colorWithRed:0.95 green:0.3 blue:0.25 alpha:1]
                                                                                                    : [UIColor whiteColor]];
}

- (void)sliderMoved {
    NSInteger p = (NSInteger)lroundf(_slider.value);
    _bottomLabel.text = [NSString stringWithFormat:@"Trang %ld / %ld", (long)p + 1, (long)_screens];
}

- (void)sliderDone {
    NSInteger p = (NSInteger)lroundf(_slider.value);
    if (p != _page) {
        [self pushBack];
        [self fadeToPage:p];
    }
}

#pragma mark - Mục lục, Aa, dấu trang

- (void)present:(UIViewController *)vc from:(UIBarButtonItem *)item {
    [_popover dismissPopoverAnimated:NO];
    _popover = nil;
    if (GBPadIdiom()) {
        UINavigationController *n = [[UINavigationController alloc] initWithRootViewController:vc];
        _popover = [[UIPopoverController alloc] initWithContentViewController:n];
        _popover.delegate = self;
        _popover.popoverContentSize = CGSizeMake(380, 600);
        [_popover presentPopoverFromBarButtonItem:item permittedArrowDirections:UIPopoverArrowDirectionAny animated:YES];
    } else {
        [self presentViewController:[[UINavigationController alloc] initWithRootViewController:vc] animated:YES completion:nil];
    }
}

- (void)popoverControllerDidDismissPopover:(UIPopoverController *)p {
    _popover = nil;
}

- (void)dismissPanel {
    if (_popover) {
        [_popover dismissPopoverAnimated:YES];
        _popover = nil;
    } else if (self.presentedViewController) {
        [self dismissViewControllerAnimated:YES completion:nil];
    }
}

- (void)showToc {
    if (!_opened) return;
    GBTocViewController *t = [[GBTocViewController alloc] initWithStyle:UITableViewStylePlain];
    t.toc = _toc;
    t.marks = _marks;
    t.currentSpine = _s;
    __weak GBReaderViewController *ws = self;
    t.pick = ^(NSDictionary *target) {
        [ws dismissPanel];
        [ws pickTarget:target];
    };
    t.removeMark = ^(NSDictionary *m) { [ws removeMark:m]; };
    [self present:t from:_tocItem];
}

- (void)pickTarget:(NSDictionary *)target {
    NSDictionary *pos = GMVal(target, @"pos");
    [self pushBack];
    if (pos) {
        [self jumpTo:pos];
        return;
    }
    NSString *frag = GMVal(target, @"frag");
    NSInteger s = [GMVal(target, @"s") integerValue];
    if (frag.length) {
        // neo có thể nằm ở phần sau của chương lớn
        [GMCore call:@"book.locate" args:@{@"id": _bookId ?: @"", @"path": GMVal([_spine objectAtIndex:s], @"path") ?: @"", @"frag": frag}
                done:^(id ok, NSDictionary *err) {
                    [self jumpTo:@{@"s": @(s), @"p": GMVal(ok, @"p") ?: @0, @"frag": frag}];
                }];
        return;
    }
    [self jumpTo:@{@"s": @(s), @"p": @0}];
}

- (void)showStyle {
    GBStyleViewController *s = [[GBStyleViewController alloc] initWithStyle:UITableViewStyleGrouped];
    __weak GBReaderViewController *ws = self;
    s.changed = ^{ [ws applyConfigKeepingPosition:YES]; };
    s.lightChanged = ^{ [ws applyLight]; };
    [self present:s from:_aaItem];
}

- (NSDictionary *)markOnPage {
    if (!_opened || _loadingChapter) return nil;
    for (NSDictionary *m in _marks) {
        NSDictionary *pos = GMVal(m, @"pos");
        if ([GMVal(pos, @"s") integerValue] != _s || [GMVal(pos, @"p") integerValue] != _p) continue;
        NSString *c = GMVal(pos, @"c");
        if (!c.length) continue;
        NSInteger page = [[self js:[NSString stringWithFormat:@"GB.screenOfPos(%@)", GBJSON(c)]] integerValue];
        if (page == _page) return m;
    }
    return nil;
}

- (void)toggleMark {
    NSDictionary *m = [self markOnPage];
    if (m) {
        [self removeMark:m];
        [GMTheme toast:@"Đã bỏ dấu trang" in:self.view];
        return;
    }
    NSDictionary *pos = [self currentPos];
    [GMCore call:@"mark.add" args:@{@"key": _key ?: @"", @"pos": pos, @"chapter": [self chapterTitle]} done:^(id ok, NSDictionary *err) {
        if (err) {
            [GMTheme toast:GMErrorText(err) in:self.view];
            return;
        }
        [GMTheme toast:@"Đã đặt dấu trang" in:self.view];
        [self reloadMarks];
    }];
}

- (void)removeMark:(NSDictionary *)m {
    [GMCore call:@"mark.remove" args:@{@"key": _key ?: @"", @"id": GMVal(m, @"id") ?: @0} done:^(id ok, NSDictionary *err) { [self reloadMarks]; }];
}

- (void)reloadMarks {
    [GMCore call:@"reading.get" args:@{@"key": _key ?: @""} done:^(id ok, NSDictionary *err) {
        _marks = GMVal(ok, @"marks") ?: @[];
        [self updateLabels];
    }];
}

#pragma mark - Lưu vị trí, giữ màn hình sáng

- (void)saveSoon {
    [_saveTimer invalidate];
    _saveTimer = [NSTimer scheduledTimerWithTimeInterval:0.6 target:self selector:@selector(savePos) userInfo:nil repeats:NO];
}

- (void)savePos {
    if (!_opened || _loadingChapter || !_key) return;
    NSDictionary *pos = [self currentPos];
    double secs = _secs;
    _secs = 0;
    [GMCore call:@"reading.save" args:@{@"key": _key, @"pos": pos, @"progress": @([self progress]), @"chapter": [self chapterTitle],
                                        @"secs": @((NSInteger)secs)} done:nil];
}

- (void)noteActivity {
    NSDate *now = [NSDate date];
    if (_lastTurn) _secs += MIN(120, [now timeIntervalSinceDate:_lastTurn]);
    _lastTurn = now;
    [self keepAwake];
}

/// Giữ màn hình sáng khi đang đọc; 10 phút không lật trang thì để máy tự khoá như thường.
- (void)keepAwake {
    [UIApplication sharedApplication].idleTimerDisabled = YES;
    [_idleTimer invalidate];
    _idleTimer = [NSTimer scheduledTimerWithTimeInterval:600 target:self selector:@selector(sleepAllowed) userInfo:nil repeats:NO];
}

- (void)sleepAllowed {
    [UIApplication sharedApplication].idleTimerDisabled = NO;
}

- (void)willResign {
    [self savePos];
    [GMCore call:@"app.flush" args:nil done:nil];
}


- (void)close {
    [self closeAnimated:YES];
}

- (void)themeChanged {
    _nightItem.image = [self nightGlyph];
    BOOL light = [@[@"day", @"sepia"] containsObject:[GBReaderPrefs theme]];
    _spin.activityIndicatorViewStyle = light ? UIActivityIndicatorViewStyleGray : UIActivityIndicatorViewStyleWhite;
    _loadLabel.textColor = [GBReaderPrefs inkFor:[GBReaderPrefs theme]];
    [self applyConfigKeepingPosition:YES];
}

- (void)closeAnimated:(BOOL)animated {
    if (_closing) return;
    [self shutdown];
    [self.presentingViewController dismissViewControllerAnimated:animated completion:nil];
}

/// Rời sách — đóng bằng nút, mở sách khác, hay bị đóng từ ngoài (viewWillDisappear): lưu vị trí, báo lõi, đóng popover, cập nhật
/// «Đang đọc» / «Đọc tiếp». Chạy một lần.
- (void)shutdown {
    if (_closing) return;
    _closing = YES;
    [GBReaderViewController rememberOpen:nil];
    [self dropPopover];
    [self savePos];
    [_idleTimer invalidate];
    [UIApplication sharedApplication].idleTimerDisabled = NO;
    if (_bookId) [GMCore call:@"book.close" args:@{@"id": _bookId} done:nil];
    else if (_key) [GMCore call:@"book.cancel" args:@{@"key": _key} done:nil];
    [_web stopLoading];
    _web.delegate = nil;
    [[NSNotificationCenter defaultCenter] postNotificationName:@"GBReadingChanged" object:nil];
}

#pragma mark - Mở lại sách sau khi iOS tắt app

/// iOS 5 / 6 hay tắt app nằm nền (thiếu RAM; WebKit giữ khoá tệp đệm shader OpenGL `shaders.maps` lúc app bị treo → 0xdead10cc —
/// DECISIONS B13): nhớ sách đang mở (khoá + tên), đóng sách thì quên; mở app lại thì mở lại đúng cuốn, đúng chỗ — như iBooks.
+ (void)rememberOpen:(NSDictionary *)book {
    NSUserDefaults *d = [NSUserDefaults standardUserDefaults];
    NSString *key = GMVal(book, @"key");
    if (key) {
        [d setObject:@{@"key": key, @"title": GMVal(book, @"title") ?: @""} forKey:@"gb.openBook"];
    } else {
        [d removeObjectForKey:@"gb.openBook"];
    }
    [d synchronize];
}

+ (NSDictionary *)bookToRestore {
    NSUserDefaults *d = [NSUserDefaults standardUserDefaults];
    NSDictionary *b = [d dictionaryForKey:@"gb.openBook"];
    if (!GMVal(b, @"key")) return nil;
    // Lần mở lại trước chưa tới được trang đầu (app chết khi mở sách đó?) → thôi, khỏi kẹt vòng mở lại → chết
    if ([d integerForKey:@"gb.restoreTries"] >= 1) {
        [d removeObjectForKey:@"gb.openBook"];
        [d setInteger:0 forKey:@"gb.restoreTries"];
        [d synchronize];
        return nil;
    }
    [d setInteger:1 forKey:@"gb.restoreTries"];
    [d synchronize];
    return b;
}

#pragma mark - Thử (GMDebug)

- (NSString *)debug:(NSString *)cmd arg:(NSString *)arg {
    if ([cmd isEqual:@"next"]) [self turn:1];
    else if ([cmd isEqual:@"prev"]) [self turn:-1];
    else if ([cmd isEqual:@"chapter"]) [self jumpTo:@{@"s": @([arg integerValue]), @"p": @0}];
    else if ([cmd isEqual:@"toc"]) [self showToc];
    else if ([cmd isEqual:@"aa"]) [self showStyle];
    else if ([cmd isEqual:@"chrome"]) [self toggleChrome];
    else if ([cmd isEqual:@"close"]) [self close];
    else if ([cmd isEqual:@"turn"]) [GBReaderPrefs setTurn:arg.length ? arg : @"curl"];
    else if ([cmd isEqual:@"size"]) {
        // như bấm A− / A+ trong bảng Aa
        [GBReaderPrefs setSizeIndex:MAX(0, MIN((NSInteger)[GBReaderPrefs sizes].count - 1, [GBReaderPrefs sizeIndex] + [arg integerValue]))];
        [self applyConfigKeepingPosition:YES];
    } else if ([cmd isEqual:@"mark"]) [self toggleMark];
    else if ([cmd isEqual:@"warm"]) {
        [GBReaderPrefs setWarm:[arg floatValue]];
        [self applyLight];
    } else if ([cmd isEqual:@"night"]) [self toggleNight];
    else if ([cmd isEqual:@"dark"]) {
        // như gạt «Giao diện tối» trong bảng Aa đang mở (iPad: popover; iPhone: màn bật lên)
        UIViewController *c = _popover ? _popover.contentViewController : self.presentedViewController;
        UIViewController *top = [c isKindOfClass:[UINavigationController class]] ? [(UINavigationController *)c topViewController] : c;
        SEL sel = NSSelectorFromString(@"darkChanged:");
        if ([top respondsToSelector:sel]) {
            UISwitch *sw = [[UISwitch alloc] init];
            sw.on = [arg isEqual:@"1"];
#pragma clang diagnostic push
#pragma clang diagnostic ignored "-Warc-performSelector-leaks"
            [top performSelector:sel withObject:sw];
#pragma clang diagnostic pop
        }
    }
    else if ([cmd isEqual:@"js"]) return [self js:arg];
    return [NSString stringWithFormat:@"chương %ld phần %ld/%ld trang %ld/%ld bận %d chụp %d next %d prev %d", (long)_s, (long)_p + 1, (long)_parts,
                                      (long)_page + 1, (long)_screens, _busy, _snapping, _imgNext != nil, _imgPrev != nil];
}

@end
