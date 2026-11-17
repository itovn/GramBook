#import "GMRootController.h"
#import "GBBooksViewController.h"
#import "GBCollectionsViewController.h"
#import "GBCover.h"
#import "GBReaderViewController.h"
#import "GBReadingViewController.h"
#import "GMAppDelegate.h"
#import "GMArt.h"
#import "GMCore.h"
#import "GMImages.h"
#import "GMSettingsViewController.h"
#import "GMSourcesViewController.h"
#import "GMTheme.h"
#import <QuartzCore/QuartzCore.h>

GMRootController *GMRoot(void) {
    return [(GMAppDelegate *)[UIApplication sharedApplication].delegate root];
}

#pragma mark - Thanh «Đọc tiếp»

@interface GBContinueBar : UIView
@property (nonatomic, copy) void (^tap)(void);
@property (nonatomic, strong) NSDictionary *book;
@end

@implementation GBContinueBar {
    UIImageView *_bg, *_cover;
    UILabel *_title, *_sub;
    UIView *_bar, *_barFill;
    NSString *_coverKey;
}

- (instancetype)initWithFrame:(CGRect)frame {
    if ((self = [super initWithFrame:frame])) {
        _bg = [[UIImageView alloc] initWithImage:[GMTheme glassBar]];
        _bg.frame = self.bounds;
        _bg.autoresizingMask = UIViewAutoresizingFlexibleWidth | UIViewAutoresizingFlexibleHeight;
        [self addSubview:_bg];
        _cover = [[UIImageView alloc] initWithFrame:CGRectMake(10, 5, 28, 42)];
        [self addSubview:_cover];
        _title = [[UILabel alloc] init];
        _title.font = [UIFont boldSystemFontOfSize:14];
        _title.textColor = [GMTheme text];
        _title.backgroundColor = [UIColor clearColor];
        [self addSubview:_title];
        _sub = [[UILabel alloc] init];
        _sub.font = [UIFont systemFontOfSize:12];
        _sub.textColor = [GMTheme secondaryText];
        _sub.backgroundColor = [UIColor clearColor];
        [self addSubview:_sub];
        _bar = [[UIView alloc] init];
        _bar.backgroundColor = [[GMTheme accent] colorWithAlphaComponent:0.18];
        [self addSubview:_bar];
        _barFill = [[UIView alloc] init];
        _barFill.backgroundColor = [GMTheme accent];
        [_bar addSubview:_barFill];
        [self addGestureRecognizer:[[UITapGestureRecognizer alloc] initWithTarget:self action:@selector(tapped)]];
    }
    return self;
}

- (void)tapped {
    if (_tap) _tap();
}

- (void)layoutSubviews {
    [super layoutSubviews];
    CGFloat w = self.bounds.size.width;
    _title.frame = CGRectMake(48, 8, w - 60, 18);
    _sub.frame = CGRectMake(48, 27, w - 60, 16);
    _bar.frame = CGRectMake(0, 0, w, 2);
    double p = [GMVal(_book, @"progress") doubleValue];
    _barFill.frame = CGRectMake(0, 0, floor(w * MIN(1, MAX(0, p))), 2);
}

- (void)setBook:(NSDictionary *)b {
    _book = b;
    _title.text = GMVal(b, @"title") ?: @"";
    NSString *ch = GMVal(b, @"chapter");
    double p = [GMVal(b, @"progress") doubleValue];
    _sub.text = ch.length ? [NSString stringWithFormat:@"Đọc tiếp · %@ · %.0f%%", ch, p * 100] : [NSString stringWithFormat:@"Đọc tiếp · %.0f%%", p * 100];
    NSString *key = GMVal(b, @"key");
    if (![key isEqual:_coverKey]) {
        _coverKey = key;
        CGSize cs = CGSizeMake(28, 42);
        _cover.image = [GBCover placeholderFor:b size:cs];
        if (key) {
            [GMImages cover:b size:42 done:^(UIImage *img) {
                if ([_coverKey isEqual:key]) _cover.image = [GBCover frame:img size:cs];
            }];
        }
    }
    [self setNeedsLayout];
}

@end

#pragma mark - Khung chính

@implementation GMRootController {
    NSArray *_navs;
    UIView *_content;
    UITabBar *_tabBar;
    GBContinueBar *_cont;
    UIViewController *_shown;
    GBBooksViewController *_library;
    BOOL _builtDark;  // khung dựng theo giao diện nào
}

- (instancetype)init {
    if ((self = [super init])) {
        UINavigationController *rd =
            [[UINavigationController alloc] initWithRootViewController:[[GBReadingViewController alloc] initWithStyle:UITableViewStylePlain]];
        rd.tabBarItem = [GMTheme tabItem:@"Đang đọc" icon:@"reading"];
        _library = [[GBBooksViewController alloc] initWithKind:GBBooksLibrary];
        UINavigationController *lib = [[UINavigationController alloc] initWithRootViewController:_library];
        lib.tabBarItem = [GMTheme tabItem:@"Thư viện" icon:@"shelf"];
        UINavigationController *src =
            [[UINavigationController alloc] initWithRootViewController:[[GMSourcesViewController alloc] initWithStyle:UITableViewStylePlain]];
        src.tabBarItem = [GMTheme tabItem:@"Nguồn" icon:@"sources"];
        UINavigationController *col =
            [[UINavigationController alloc] initWithRootViewController:[[GBCollectionsViewController alloc] initWithStyle:UITableViewStylePlain]];
        col.tabBarItem = [GMTheme tabItem:@"Bộ sưu tập" icon:@"collections"];
        UINavigationController *st =
            [[UINavigationController alloc] initWithRootViewController:[[GMSettingsViewController alloc] initWithStyle:UITableViewStyleGrouped]];
        st.tabBarItem = [GMTheme tabItem:@"Cài đặt" icon:@"settings"];
        _navs = @[rd, lib, src, col, st];
        _selectedIndex = 1;
        _builtDark = [GMTheme dark];
    }
    return self;
}

- (void)viewDidLoad {
    [super viewDidLoad];
    self.view.backgroundColor = [GMTheme background];
    [self.view addSubview:[[GMWallpaperView alloc] initWithFrame:self.view.bounds]];
    _content = [[UIView alloc] initWithFrame:self.view.bounds];
    _content.backgroundColor = [UIColor clearColor];
    [self.view addSubview:_content];
    _cont = [[GBContinueBar alloc] initWithFrame:CGRectMake(0, 0, self.view.bounds.size.width, 52)];
    __weak GMRootController *ws = self;
    _cont.tap = ^{ [ws continueReading]; };
    _cont.hidden = YES;
    [self.view addSubview:_cont];
    _tabBar = [[UITabBar alloc] init];
    _tabBar.delegate = self;
    NSMutableArray *items = [NSMutableArray array];
    for (UIViewController *vc in _navs) [items addObject:vc.tabBarItem];
    _tabBar.items = items;
    [self.view addSubview:_tabBar];
    [[NSNotificationCenter defaultCenter] addObserver:self selector:@selector(refreshContinue) name:@"GBReadingChanged" object:nil];
    [self show:_selectedIndex];
    [self refreshContinue];
}

- (void)dealloc {
    [[NSNotificationCenter defaultCenter] removeObserver:self];
}

- (void)viewDidAppear:(BOOL)animated {
    [super viewDidAppear:animated];
    // Giao diện đổi lúc đang đọc (bảng Aa, tự động theo giờ): đóng sách xong mới dựng lại khung theo giao diện mới
    if (_builtDark != [GMTheme dark] && !self.presentedViewController) {
        dispatch_async(dispatch_get_main_queue(), ^{
            [(GMAppDelegate *)[UIApplication sharedApplication].delegate reloadTheme];
        });
    }
}

- (BOOL)shouldAutorotateToInterfaceOrientation:(UIInterfaceOrientation)o {
    return UI_USER_INTERFACE_IDIOM() == UIUserInterfaceIdiomPad || o == UIInterfaceOrientationPortrait;
}

- (BOOL)shouldAutorotate {
    return YES;
}

- (UIInterfaceOrientationMask)supportedInterfaceOrientations {
    return (UIInterfaceOrientationMask)(UI_USER_INTERFACE_IDIOM() == UIUserInterfaceIdiomPad ? 30 : 2);  // số của UIInterfaceOrientationMask (iOS 6)
}

- (void)viewWillLayoutSubviews {
    [super viewWillLayoutSubviews];
    CGRect b = self.view.bounds;
    CGFloat tabH = 49;
    CGFloat contH = _cont.hidden ? 0 : 52;
    _tabBar.frame = CGRectMake(0, b.size.height - tabH, b.size.width, tabH);
    _cont.frame = CGRectMake(0, b.size.height - tabH - 52, b.size.width, 52);
    _content.frame = CGRectMake(0, 0, b.size.width, b.size.height - tabH - contH);
    _shown.view.frame = _content.bounds;
}

/// Thanh «Đọc tiếp»: sách đọc dở mở gần nhất.
- (void)refreshContinue {
    [GMCore call:@"reading.recent" args:@{@"limit": @1, @"finished": @NO} done:^(id ok, NSDictionary *err) {
        NSDictionary *r = [ok isKindOfClass:[NSArray class]] && [ok count] ? [ok objectAtIndex:0] : nil;
        BOOL want = r != nil;
        if (r) _cont.book = r;
        if (want == _cont.hidden) {
            _cont.hidden = !want;
            [self.view setNeedsLayout];
        }
    }];
}

- (void)continueReading {
    NSDictionary *b = _cont.book;
    if (b) [self openBook:b];
}

- (void)setSelectedIndex:(NSInteger)i {
    _selectedIndex = MAX(0, MIN(i, 4));
    if (self.isViewLoaded) [self show:_selectedIndex];
}

- (void)show:(NSInteger)i {
    UIViewController *vc = [_navs objectAtIndex:i];
    _tabBar.selectedItem = [_tabBar.items objectAtIndex:i];
    if (vc == _shown) return;
    if (_shown) {
        [_shown willMoveToParentViewController:nil];
        [_shown.view removeFromSuperview];
        [_shown removeFromParentViewController];
    }
    [self addChildViewController:vc];
    vc.view.frame = _content.bounds;
    vc.view.autoresizingMask = UIViewAutoresizingFlexibleWidth | UIViewAutoresizingFlexibleHeight;
    [_content addSubview:vc.view];
    [vc didMoveToParentViewController:self];
    _shown = vc;
}

- (void)tabBar:(UITabBar *)tabBar didSelectItem:(UITabBarItem *)item {
    NSInteger i = [tabBar.items indexOfObject:item];
    if (i == NSNotFound) return;
    UINavigationController *nav = [_navs objectAtIndex:i];
    if (i == _selectedIndex && _shown == nav) {
        [nav popToRootViewControllerAnimated:YES];
        return;
    }
    self.selectedIndex = i;
}

- (void)loggedIn:(NSDictionary *)me {
    _me = me;
    [_library loggedIn];
}

- (void)loggedOut {
    _me = nil;
}

- (GBReaderViewController *)reader {
    UIViewController *p = self.presentedViewController;
    return [p isKindOfClass:[GBReaderViewController class]] ? (GBReaderViewController *)p : nil;
}

- (void)openBook:(NSDictionary *)book {
    if (!GMVal(book, @"key")) return;
    GBReaderViewController *old = self.reader;
    if (old) [old closeAnimated:NO];  // lưu vị trí + book.close, đóng popover (không thì UIKit ném lỗi khi giải phóng)
    else if (self.presentedViewController) [self dismissViewControllerAnimated:NO completion:nil];
    GBReaderViewController *r = [[GBReaderViewController alloc] initWithBook:book];
    r.modalTransitionStyle = UIModalTransitionStyleCrossDissolve;
    [self presentViewController:r animated:YES completion:nil];
}

@end
