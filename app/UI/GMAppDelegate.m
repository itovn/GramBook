#import "GMAppDelegate.h"
#import "GBReaderViewController.h"
#import "GMCompat.h"
#import "GMCore.h"
#import "GMLog.h"
#import "GMLoginViewController.h"
#import "GMRootController.h"
#import "GMTheme.h"
#ifdef DEBUG
#import "GMDebug.h"
#endif

@interface GMAppDelegate () <UIAlertViewDelegate>
@end

@implementation GMAppDelegate {
    BOOL _loginShown, _deadShown;
}

- (BOOL)application:(UIApplication *)application didFinishLaunchingWithOptions:(NSDictionary *)launchOptions {
    NSString *version = [[NSBundle mainBundle] objectForInfoDictionaryKey:@"CFBundleShortVersionString"];
    GMLog(@"=== GramBook %@ — iOS %@%@, RAM %.0f MB", version, [UIDevice currentDevice].systemVersion,
          GMOldIOS() ? @" (kiểu iOS 5)" : @"", [NSProcessInfo processInfo].physicalMemory / 1048576.0);
    [GMTheme apply];
    BOOL started = [GMCore start];

    self.window = [[UIWindow alloc] initWithFrame:[UIScreen mainScreen].bounds];
    [self buildUI];
    [self.window makeKeyAndVisible];

    if (!started) {
        [[[UIAlertView alloc] initWithTitle:@"Không mở được lõi" message:@"Xem app.log trong ~/Library/GramBook." delegate:nil
                          cancelButtonTitle:@"OK" otherButtonTitles:nil] show];
        return YES;
    }
    [[NSNotificationCenter defaultCenter] addObserver:self selector:@selector(event:) name:GMCoreEventNotification object:nil];
    [self checkAuth];
#ifdef DEBUG
    [GMDebug start];
#endif
    return YES;
}

- (void)checkAuth {
    [GMCore call:@"auth.state" args:nil done:^(id ok, NSDictionary *err) {
        if (err) {
            // Mạng lỗi (bị chặn?): vẫn vào thư viện (sách đã quét / đã tải vẫn đọc được); Cài đặt → Proxy
            GMLog(@"auth.state lỗi: %@", [err objectForKey:@"code"]);
            [self restoreReading];
            return;
        }
        if ([GMVal(ok, @"authorized") boolValue]) {
            [self.root loggedIn:GMVal(ok, @"me")];
            [self restoreReading];
        } else if ([GMVal(ok, @"dead") boolValue]) {
            [self showDead];
        } else {
            [self showLogin];
        }
    }];
}

/// iOS tắt app lúc đang đọc (nằm nền) → mở lại đúng cuốn đó (màn đọc tự về chỗ đã lưu).
- (void)restoreReading {
    NSDictionary *b = [GBReaderViewController bookToRestore];
    if (!b || self.root.presentedViewController) return;
    GMLog(@"mở lại sách đang đọc dở: %@", GMVal(b, @"title"));
    [self.root openBook:b];
}

- (void)loginClosed {
    _loginShown = NO;
}

- (void)showLogin {
    if (_loginShown) return;
    _loginShown = YES;
    UINavigationController *n = [[UINavigationController alloc] initWithRootViewController:[[GMLoginViewController alloc] init]];
    [self.root presentViewController:n animated:YES completion:nil];
}

/// Telegram huỷ phiên (dùng trùng ở hai nơi / bị thu hồi — GramMini D24): lõi đã dời tệp phiên → mở lại app để đăng nhập.
- (void)showDead {
    if (_deadShown) return;
    _deadShown = YES;
    GMLog(@"phiên bị Telegram huỷ — chờ mở lại app để đăng nhập");
    [[[UIAlertView alloc] initWithTitle:@"Cần đăng nhập lại"
                                message:@"Telegram đã huỷ phiên đăng nhập của GramBook trên máy này. Tài khoản của bạn vẫn bình "
                                        @"thường. Bấm «Mở lại» rồi chạm biểu tượng GramBook để đăng nhập."
                               delegate:self cancelButtonTitle:nil otherButtonTitles:@"Mở lại", nil] show];
}

- (void)alertView:(UIAlertView *)alertView didDismissWithButtonIndex:(NSInteger)buttonIndex {
    if (_deadShown) exit(0);
}

- (void)event:(NSNotification *)n {
    NSString *e = GMVal(n.userInfo, @"e");
    if ([e isEqual:@"auth.dead"]) {
        [self showDead];
        return;
    }
    if (![e isEqual:@"auth.state"] || [GMVal(n.userInfo, @"dead") boolValue]) return;
    if ([GMVal(n.userInfo, @"authorized") boolValue]) {
        if (!_loginShown) return;  // lúc mở app: checkAuth đã lo
        _loginShown = NO;
        [GMCore call:@"auth.state" args:nil done:^(id ok, NSDictionary *err) {
            [self.root loggedIn:GMVal(ok, @"me")];
        }];
    } else {
        [self.root loggedOut];
        [self showLogin];
    }
}

- (void)buildUI {
    self.window.backgroundColor = [GMTheme background];
    NSInteger tab = self.root ? self.root.selectedIndex : 0;
    NSDictionary *me = self.root.me;
    self.root = [[GMRootController alloc] init];
    self.root.selectedIndex = tab;
    self.window.rootViewController = self.root;
    if (me) [self.root loggedIn:me];
}

- (void)reloadTheme {
    // Đang đọc: đổi nền trang ngay; khung app (sau màn đọc) dựng lại khi đóng sách — GMRootController viewDidAppear
    GBReaderViewController *reader = self.root.reader;
    if (reader) {
        [GMTheme apply];
        [reader themeChanged];
        return;
    }
    // Màn đang bật lên (Đăng nhập…) thuộc khung cũ: đóng trước khi thay khung, rồi mở lại màn đăng nhập nếu cần
    BOOL login = _loginShown;
    if (self.root.presentedViewController) [self.root dismissViewControllerAnimated:NO completion:nil];
    _loginShown = NO;
    [GMTheme apply];
    [self buildUI];
    if (login) [self showLogin];
}

- (void)applicationDidBecomeActive:(UIApplication *)application {
    if ([GMTheme wantsDark] != [GMTheme dark]) [self reloadTheme];
    [GMCore call:@"net.check" args:nil done:nil];
    // Nằm nền lâu: iOS thu hồi cổng 127.0.0.1 phục vụ sách → mở lại trước khi màn đọc nạp chương (GramMusic M23)
    [GMCore call:@"serve.revive" args:@{} done:^(id ok, NSDictionary *err) {
        if ([[ok objectForKey:@"revived"] boolValue]) {
            GMLog(@"sách: cổng bị iOS thu hồi lúc nằm nền — đã mở lại cổng %@", [ok objectForKey:@"port"]);
        }
    }];
}

- (void)applicationDidEnterBackground:(UIApplication *)application {
    [GMCore call:@"app.flush" args:nil done:nil];
}

- (void)applicationWillTerminate:(UIApplication *)application {
    [GMCore call:@"app.flush" args:nil done:nil];
}

@end
