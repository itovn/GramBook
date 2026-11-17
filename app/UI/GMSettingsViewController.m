#import "GMSettingsViewController.h"
#import "GMAppDelegate.h"
#import "GBReaderParts.h"
#import "GMCore.h"
#import "GMImages.h"
#import "GMProxyViewController.h"
#import "GMRootController.h"
#import "GMTheme.h"

/// Bảng chọn dùng khối (UIActionSheet không giữ delegate).
@interface GMChoice : NSObject <UIActionSheetDelegate, UIAlertViewDelegate>
@property (nonatomic, copy) void (^pick)(NSInteger i);
@end

static NSMutableSet *gChoices;

@implementation GMChoice
- (void)actionSheet:(UIActionSheet *)s clickedButtonAtIndex:(NSInteger)i {
    if (i != s.cancelButtonIndex && i >= 0 && _pick) _pick(i);
}
- (void)actionSheet:(UIActionSheet *)s didDismissWithButtonIndex:(NSInteger)i {
    [gChoices removeObject:self];
}
- (void)alertView:(UIAlertView *)a clickedButtonAtIndex:(NSInteger)i {
    if (i != a.cancelButtonIndex && _pick) _pick(i);
}
- (void)alertView:(UIAlertView *)a didDismissWithButtonIndex:(NSInteger)i {
    [gChoices removeObject:self];
}
@end

static void GMChoose(UIViewController *vc, NSString *title, NSArray *options, void (^pick)(NSInteger i)) {
    if (!gChoices) gChoices = [NSMutableSet set];
    GMChoice *c = [[GMChoice alloc] init];
    c.pick = pick;
    UIActionSheet *s = [[UIActionSheet alloc] initWithTitle:title delegate:c cancelButtonTitle:nil destructiveButtonTitle:nil
                                          otherButtonTitles:nil];
    for (NSString *o in options) [s addButtonWithTitle:o];
    s.cancelButtonIndex = [s addButtonWithTitle:@"Huỷ"];
    [gChoices addObject:c];
    [s showInView:vc.view.window ?: vc.view];
}

static void GMConfirm(NSString *title, NSString *msg, NSString *ok, void (^yes)(void)) {
    if (!gChoices) gChoices = [NSMutableSet set];
    GMChoice *c = [[GMChoice alloc] init];
    c.pick = ^(NSInteger i) { yes(); };
    [gChoices addObject:c];
    [[[UIAlertView alloc] initWithTitle:title message:msg delegate:c cancelButtonTitle:@"Huỷ" otherButtonTitles:ok, nil] show];
}

#pragma mark - Cài đặt

/// Các nhóm của màn Cài đặt.
enum { GMSecAccount, GMSecLibrary, GMSecPlayback, GMSecApp, GMSecAbout, GMSecLogout, GMSecCount };

@implementation GMSettingsViewController {
    NSDictionary *_stats;
    NSDictionary *_prefs;
}

- (void)viewDidLoad {
    [super viewDidLoad];
    self.title = @"Cài đặt";
}

- (void)viewWillAppear:(BOOL)animated {
    [super viewWillAppear:animated];
    [GMCore call:@"library.stats" args:nil done:^(id ok, NSDictionary *err) {
        if (!err) _stats = ok;
        [self.tableView reloadData];
    }];
    [self loadPrefs];
}

- (void)loadPrefs {
    [GMCore call:@"settings.get" args:nil done:^(id ok, NSDictionary *err) {
        if (!err) _prefs = ok;
        [self.tableView reloadData];
    }];
}

- (void)setPref:(NSString *)key value:(id)v {
    [GMCore call:@"settings.set" args:@{key: v} done:^(id ok, NSDictionary *err) {
        if (err) [GMTheme toast:GMErrorText(err) in:self.view];
        [self loadPrefs];
    }];
}

- (NSInteger)numberOfSectionsInTableView:(UITableView *)tv {
    return GMSecCount;
}

- (NSInteger)tableView:(UITableView *)tv numberOfRowsInSection:(NSInteger)s {
    switch (s) {
        case GMSecAccount: return 1;
        case GMSecLibrary: return 2;
        case GMSecPlayback: return 1;
        case GMSecApp: return 4;
        case GMSecAbout: return 1;
        default: return GMRoot().me ? 1 : 0;
    }
}

- (NSString *)tableView:(UITableView *)tv titleForHeaderInSection:(NSInteger)s {
    if (s == GMSecLibrary) return @"Thư viện";
    if (s == GMSecPlayback) return @"Đọc sách";
    return nil;
}

- (NSString *)tableView:(UITableView *)tv titleForFooterInSection:(NSInteger)s {
    if (s == GMSecPlayback) {
        return @"Cỡ chữ, font, nền, giãn dòng, lề, kiểu lật trang (cong như iBooks / trượt / không hiệu ứng) — đổi được cả khi đang đọc "
               @"(nút «Aa»).";
    }
    if (s == GMSecApp) {
        NSString *prov = GMVal(_prefs, @"dns_providers") ?: @"Cloudflare → Google";
        return [NSString stringWithFormat:@"DNS mã hoá: tên miền của proxy được hỏi qua %@ (DoH, rồi DoT) — nhà mạng "
                                          @"không đọc / sửa được; lỗi hết mới dùng DNS của máy.", prov];
    }
    if (s == GMSecLibrary) {
        NSInteger total = [GMVal(_stats, @"total") integerValue];
        NSNumber *last = GMVal(_stats, @"last_sync");
        NSString *when = last.longLongValue > 0 ? [NSString stringWithFormat:@", quét lần cuối %@ %@", [GMTheme shortTime:last],
                                                   [GMTheme clock:last]] : @"";
        NSString *partial = [GMVal(_stats, @"partial") boolValue] ? @" Lần quét toàn bộ trước còn dở — sẽ quét tiếp." : @"";
        return [NSString stringWithFormat:@"%ld cuốn từ %@ nguồn%@.%@", (long)total, GMVal(_stats, @"sources") ?: @0, when, partial];
    }
    if (s == GMSecAbout) {
        return @"GramBook — đọc sách / ebook lưu trên Telegram trên iOS 5 / 6. Ứng dụng không chính thức, dùng Telegram API; không liên "
               @"kết với Telegram. Phiên đăng nhập, thư viện và vị trí đọc chỉ nằm trên máy này. Font Literata: SIL Open Font License.";
    }
    return nil;
}

- (UITableViewCell *)tableView:(UITableView *)tv cellForRowAtIndexPath:(NSIndexPath *)ip {
    UITableViewCell *c = [[UITableViewCell alloc] initWithStyle:UITableViewCellStyleValue1 reuseIdentifier:nil];
    c.accessoryType = UITableViewCellAccessoryDisclosureIndicator;
    if (ip.section == GMSecAccount) {
        NSDictionary *me = GMRoot().me;
        if (me) {
            c = [[UITableViewCell alloc] initWithStyle:UITableViewCellStyleSubtitle reuseIdentifier:nil];
            c.textLabel.text = GMVal(me, @"name");
            NSString *u = GMVal(me, @"username");
            NSString *phone = GMVal(me, @"phone");
            c.detailTextLabel.text = u.length ? [@"@" stringByAppendingString:u] : (phone.length ? [@"+" stringByAppendingString:phone] : @"");
            c.imageView.image = [GMTheme avatarFor:GMVal(me, @"id") name:GMVal(me, @"name") size:40];
            NSString *photo = GMVal(me, @"photo");
            if (photo) {
                __weak UITableViewCell *wc = c;
                [GMImages avatarForPeer:GMVal(me, @"id") photo:photo dc:GMVal(me, @"photo_dc") size:40 done:^(UIImage *img) {
                    wc.imageView.image = img;
                    [wc setNeedsLayout];
                }];
            }
            c.selectionStyle = UITableViewCellSelectionStyleNone;
            c.accessoryType = UITableViewCellAccessoryNone;
        } else {
            c.textLabel.text = @"Đăng nhập Telegram";
            c.textLabel.textColor = [GMTheme accent];
        }
    } else if (ip.section == GMSecLibrary) {
        c.textLabel.text = ip.row == 0 ? @"Lấy sách mới" : @"Quét toàn bộ tài khoản";
        c.imageView.image = [GMTheme settingsIcon:ip.row == 0 ? @"folders" : @"saved"];
        c.accessoryType = UITableViewCellAccessoryNone;
    } else if (ip.section == GMSecPlayback) {
        c.textLabel.text = @"Kiểu chữ, nền, lật trang";
        NSArray *f = [[GBReaderPrefs fonts] objectAtIndex:[GBReaderPrefs fontIndex]];
        c.detailTextLabel.text = [f objectAtIndex:0];
        c.imageView.image = [GMTheme settingsIcon:@"theme"];
    } else if (ip.section == GMSecApp) {
        if (ip.row == 0) {
            c.textLabel.text = @"Proxy";
            c.imageView.image = [GMTheme settingsIcon:@"proxy"];
        } else if (ip.row == 1) {
            c.textLabel.text = @"DNS mã hoá";
            c.detailTextLabel.text = !_prefs ? @"" : ([GMVal(_prefs, @"secure_dns") boolValue] ? @"Bật" : @"Tắt");
            c.imageView.image = [GMTheme settingsIcon:@"lock"];
        } else if (ip.row == 2) {
            c.textLabel.text = @"Dữ liệu và bộ nhớ";
            c.imageView.image = [GMTheme settingsIcon:@"storage"];
        } else {
            c.textLabel.text = @"Giao diện";
            c.detailTextLabel.text = [GMTheme modeText:[GMTheme mode]];
            c.imageView.image = [GMTheme settingsIcon:@"theme"];
        }
    } else if (ip.section == GMSecAbout) {
        c.textLabel.text = @"Phiên bản";
        c.detailTextLabel.text = [[NSBundle mainBundle] objectForInfoDictionaryKey:@"CFBundleShortVersionString"];
        c.imageView.image = [GMTheme settingsIcon:@"info"];
        c.accessoryType = UITableViewCellAccessoryNone;
        c.selectionStyle = UITableViewCellSelectionStyleNone;
    } else {
        c = [[UITableViewCell alloc] initWithStyle:UITableViewCellStyleDefault reuseIdentifier:nil];
        c.textLabel.text = @"Đăng xuất";
        c.textLabel.textAlignment = GMAlignCenter;
        c.textLabel.textColor = [UIColor colorWithRed:0.85 green:0.2 blue:0.2 alpha:1];
    }
    return c;
}

- (void)tableView:(UITableView *)tv didSelectRowAtIndexPath:(NSIndexPath *)ip {
    [tv deselectRowAtIndexPath:ip animated:YES];
    GMAppDelegate *app = (GMAppDelegate *)[UIApplication sharedApplication].delegate;
    if (ip.section == GMSecAccount && !GMRoot().me) {
        [app showLogin];
    } else if (ip.section == GMSecPlayback) {
        // Chỉ lưu (NSUserDefaults) — màn đọc gửi cấu hình cho lõi lúc mở sách
        GBStyleViewController *st = [[GBStyleViewController alloc] initWithStyle:UITableViewStyleGrouped];
        [self.navigationController pushViewController:st animated:YES];
    } else if (ip.section == GMSecLibrary) {
        if (ip.row == 0) {
            [GMCore call:@"sync.start" args:@{} done:^(id ok, NSDictionary *err) {
                [GMTheme toast:[GMVal(ok, @"started") boolValue] ? @"Đang tìm sách mới…" : @"Đang quét rồi." in:self.view];
            }];
        } else {
            GMConfirm(@"Quét toàn bộ tài khoản?", @"Đọc lại mọi tệp sách trong các chat, nhóm, kênh. Có thể mất vài phút.", @"Quét", ^{
                [GMCore call:@"sync.start" args:@{@"full": @YES} done:^(id ok, NSDictionary *err) {
                    [GMTheme toast:[GMVal(ok, @"started") boolValue] ? @"Đang quét toàn bộ…" : @"Đang quét rồi." in:self.view];
                }];
            });
        }
    } else if (ip.section == GMSecApp) {
        if (ip.row == 0) {
            [self.navigationController pushViewController:[[GMProxyViewController alloc] init] animated:YES];
        } else if (ip.row == 1) {
            GMChoose(self, @"DNS mã hoá (DoH / DoT)", @[@"Bật (nên dùng)", @"Tắt — dùng DNS của máy"], ^(NSInteger i) {
                [self setPref:@"secure_dns" value:@(i == 0)];
            });
        } else if (ip.row == 2) {
            [self.navigationController pushViewController:[[GMStorageViewController alloc] initWithStyle:UITableViewStyleGrouped]
                                                 animated:YES];
        } else {
            GMChoose(self, @"Giao diện", @[@"Sáng", @"Tối", @"Tự động (tối từ 19:00 tới 6:00)"], ^(NSInteger i) {
                [GMTheme setMode:i];
                [app reloadTheme];
            });
        }
    } else if (ip.section == GMSecLogout) {
        GMConfirm(@"Đăng xuất?", @"Sách đã tải về máy và vị trí đọc vẫn giữ. Đăng nhập lại để đọc tiếp sách từ Telegram.", @"Đăng xuất", ^{
            [GMCore call:@"auth.logout" args:nil done:nil];
        });
    }
}

@end

#pragma mark - Dữ liệu và bộ nhớ

@implementation GMStorageViewController {
    NSDictionary *_st;
}

static NSArray *GMKinds(void) {
    return @[@"offline", @"books", @"cover", @"avatar"];
}

static NSString *GMKindName(NSString *k) {
    if ([k isEqual:@"offline"]) return @"Sách đã tải về máy";
    if ([k isEqual:@"books"]) return @"Sách đã mở (bộ nhớ đệm)";
    if ([k isEqual:@"cover"]) return @"Bìa sách";
    return @"Ảnh nguồn";
}

- (void)viewDidLoad {
    [super viewDidLoad];
    self.title = @"Dữ liệu và bộ nhớ";
    [self load];
}

- (void)load {
    [GMCore call:@"cache.stats" args:nil done:^(id ok, NSDictionary *err) {
        if (err) {
            [GMTheme toast:GMErrorText(err) in:self.view];
            return;
        }
        _st = ok;
        [self.tableView reloadData];
    }];
}

- (NSInteger)numberOfSectionsInTableView:(UITableView *)tv {
    return 3;
}

- (NSInteger)tableView:(UITableView *)tv numberOfRowsInSection:(NSInteger)s {
    return s == 0 ? GMKinds().count : (s == 1 ? 2 : 1);
}

- (NSString *)tableView:(UITableView *)tv titleForHeaderInSection:(NSInteger)s {
    if (s == 0 && _st) {
        NSNumber *free = GMVal(_st, @"free");
        return [NSString stringWithFormat:@"Đang dùng %@%@", [GMTheme sizeText:[GMVal(_st, @"total") doubleValue]],
                free ? [NSString stringWithFormat:@" · máy còn trống %@", [GMTheme sizeText:free.doubleValue]] : @""];
    }
    return nil;
}

- (NSString *)tableView:(UITableView *)tv titleForFooterInSection:(NSInteger)s {
    if (s == 0) return @"Chạm một dòng để xoá. Sách đã tải về máy không bao giờ tự xoá; vị trí đọc, dấu trang không bao giờ bị xoá.";
    if (s == 1) return @"Sách đã mở được giữ để lần sau đọc không cần mạng. Đầy giới hạn thì tự xoá sách lâu không mở nhất (trừ sách "
                       @"đang đọc dở); máy sắp hết chỗ cũng tự dọn.";
    return nil;
}

static NSString *GMLimitText(NSInteger mb) {
    if (mb <= 0) return @"Không giới hạn";
    return mb >= 1024 ? [NSString stringWithFormat:@"%ld GB", (long)(mb / 1024)] : [NSString stringWithFormat:@"%ld MB", (long)mb];
}

static NSString *GMKeepText(NSInteger d) {
    if (d <= 0) return @"Mãi mãi";
    if (d == 3) return @"3 ngày";
    if (d == 7) return @"1 tuần";
    if (d == 30) return @"1 tháng";
    return [NSString stringWithFormat:@"%ld ngày", (long)d];
}

- (UITableViewCell *)tableView:(UITableView *)tv cellForRowAtIndexPath:(NSIndexPath *)ip {
    UITableViewCell *c = [[UITableViewCell alloc] initWithStyle:UITableViewCellStyleValue1 reuseIdentifier:nil];
    if (ip.section == 0) {
        NSString *k = [GMKinds() objectAtIndex:ip.row];
        c.textLabel.text = GMKindName(k);
        c.detailTextLabel.text = [GMTheme sizeText:[GMVal(GMVal(_st, @"kinds"), k) doubleValue]];
    } else if (ip.section == 1) {
        if (ip.row == 0) {
            c.textLabel.text = @"Giới hạn bộ nhớ đệm";
            c.detailTextLabel.text = GMLimitText([GMVal(_st, @"limit_mb") integerValue]);
        } else {
            c.textLabel.text = @"Giữ sách đã mở";
            c.detailTextLabel.text = GMKeepText([GMVal(_st, @"keep_days") integerValue]);
        }
        c.accessoryType = UITableViewCellAccessoryDisclosureIndicator;
    } else {
        c.textLabel.text = @"Xoá bộ nhớ đệm";
        c.textLabel.textColor = [UIColor colorWithRed:0.85 green:0.2 blue:0.2 alpha:1];
    }
    return c;
}

- (void)clear:(NSArray *)kinds title:(NSString *)title {
    GMConfirm(title, [kinds containsObject:@"offline"] ? @"Các sách này sẽ phải tải lại khi đọc." : nil, @"Xoá", ^{
        [GMCore call:@"cache.clear" args:@{@"kinds": kinds} done:^(id ok, NSDictionary *err) {
            if (!err) [GMTheme toast:[NSString stringWithFormat:@"Đã xoá %@", [GMTheme sizeText:[GMVal(ok, @"freed") doubleValue]]] in:self.view];
            [self load];
        }];
    });
}

- (void)tableView:(UITableView *)tv didSelectRowAtIndexPath:(NSIndexPath *)ip {
    [tv deselectRowAtIndexPath:ip animated:YES];
    if (ip.section == 0) {
        NSString *k = [GMKinds() objectAtIndex:ip.row];
        [self clear:@[k] title:[NSString stringWithFormat:@"Xoá «%@»?", GMKindName(k)]];
    } else if (ip.section == 1 && ip.row == 0) {
        NSArray *mb = @[@256, @512, @1024, @2048, @5120, @0];
        NSMutableArray *names = [NSMutableArray array];
        for (NSNumber *m in mb) [names addObject:GMLimitText(m.integerValue)];
        GMChoose(self, @"Giới hạn bộ nhớ đệm", names, ^(NSInteger i) {
            [GMCore call:@"cache.set" args:@{@"limit_mb": [mb objectAtIndex:i]} done:^(id ok, NSDictionary *err) { [self load]; }];
        });
    } else if (ip.section == 1) {
        NSArray *days = @[@3, @7, @30, @0];
        NSMutableArray *names = [NSMutableArray array];
        for (NSNumber *d in days) [names addObject:GMKeepText(d.integerValue)];
        GMChoose(self, @"Giữ sách đã mở", names, ^(NSInteger i) {
            [GMCore call:@"cache.set" args:@{@"keep_days": [days objectAtIndex:i]} done:^(id ok, NSDictionary *err) { [self load]; }];
        });
    } else {
        [self clear:@[@"books", @"cover", @"avatar"] title:@"Xoá bộ nhớ đệm? (sách đã tải về máy giữ nguyên)"];
    }
}

@end
