#import "GMDebug.h"
#import "GBBookActions.h"
#import "GBReaderViewController.h"
#import "GMAppDelegate.h"
#import "GMCore.h"
#import "GMLog.h"
#import "GMRootController.h"
#import "GMTheme.h"
#import "GMUpdate.h"
#import <QuartzCore/QuartzCore.h>
#import <UIKit/UIKit.h>

static NSString *const kCmdFile = @"/var/mobile/Documents/GramBook-cmd.txt";

@implementation GMDebug

+ (void)start {
    [NSTimer scheduledTimerWithTimeInterval:0.5 target:self selector:@selector(tick) userInfo:nil repeats:YES];
    // Bản thử: giữ màn hình sáng để SSH / chụp màn hình không bị mất khi máy tự khoá
    [UIApplication sharedApplication].idleTimerDisabled = YES;
    GMLog(@"[thử] đọc lệnh từ %@", kCmdFile);
}

+ (void)tick {
    NSString *s = [NSString stringWithContentsOfFile:kCmdFile encoding:NSUTF8StringEncoding error:NULL];
    if (!s) return;
    [[NSFileManager defaultManager] removeItemAtPath:kCmdFile error:NULL];
    for (NSString *raw in [s componentsSeparatedByString:@"\n"]) {
        NSString *line = [raw stringByTrimmingCharactersInSet:[NSCharacterSet whitespaceAndNewlineCharacterSet]];
        if (line.length) [self run:line];
    }
}

/// Màn trên cùng: màn đang phủ hoặc màn đầu của mục đang chọn.
+ (UIViewController *)top {
    GMRootController *root = GMRoot();
    UIViewController *vc = root.presentedViewController;
    if (!vc) {
        for (UIViewController *c in root.childViewControllers) vc = c;
    }
    if ([vc isKindOfClass:[UINavigationController class]]) vc = ((UINavigationController *)vc).topViewController;
    return vc;
}

+ (UITextField *)firstField:(UIView *)v {
    if ([v isKindOfClass:[UITextField class]] && !v.hidden) return (UITextField *)v;
    for (UIView *s in v.subviews) {
        UITextField *f = [self firstField:s];
        if (f) return f;
    }
    return nil;
}

static NSString *GMShort(id v) {
    NSData *d = v ? [NSJSONSerialization dataWithJSONObject:v options:0 error:NULL] : nil;
    NSString *s = d ? [[NSString alloc] initWithData:d encoding:NSUTF8StringEncoding] : [v description];
    return s.length > 600 ? [[s substringToIndex:600] stringByAppendingString:@"…"] : s;
}

+ (void)run:(NSString *)line {
    NSRange sp = [line rangeOfString:@" "];
    NSString *cmd = sp.location == NSNotFound ? line : [line substringToIndex:sp.location];
    NSString *arg = sp.location == NSNotFound ? @"" : [line substringFromIndex:sp.location + 1];
    // «type»: chữ gõ có thể là số điện thoại / mã đăng nhập → không ghi vào app.log
    GMLog(@"[thử] %@", [cmd isEqual:@"type"] ? [NSString stringWithFormat:@"type (%lu ký tự)", (unsigned long)arg.length] : line);
    UIViewController *top = [self top];
    if ([cmd isEqual:@"call"]) {
        NSRange s2 = [arg rangeOfString:@" "];
        NSString *m = s2.location == NSNotFound ? arg : [arg substringToIndex:s2.location];
        NSString *js = s2.location == NSNotFound ? @"{}" : [arg substringFromIndex:s2.location + 1];
        id a = [NSJSONSerialization JSONObjectWithData:[js dataUsingEncoding:NSUTF8StringEncoding] options:0 error:NULL];
        long long t = GMNowMs();
        [GMCore call:m args:[a isKindOfClass:[NSDictionary class]] ? a : @{} done:^(id ok, NSDictionary *err) {
            GMLog(@"[thử] %@ → %lld ms %@", m, GMNowMs() - t, err ? GMShort(err) : GMShort(ok));
        }];
    } else if ([cmd isEqual:@"type"]) {
        // Gõ vào ô nhập đầu tiên của màn trên cùng (đăng nhập: số điện thoại / mã / mật khẩu)
        UITextField *f = [self firstField:top.view];
        if (!f) {
            GMLog(@"[thử] type: không có ô nhập");
            return;
        }
        f.text = arg;
        [f sendActionsForControlEvents:UIControlEventEditingChanged];
    } else if ([cmd isEqual:@"pref"]) {
        // pref <khoá> <giá trị>: đặt NSUserDefaults (chuỗi; "0" / "1" → BOOL) — đổi cài đặt trước khi mở màn nào
        NSRange s2 = [arg rangeOfString:@" "];
        if (s2.location == NSNotFound) return;
        NSString *k = [arg substringToIndex:s2.location], *v = [arg substringFromIndex:s2.location + 1];
        if ([v isEqual:@"0"] || [v isEqual:@"1"]) [[NSUserDefaults standardUserDefaults] setBool:[v isEqual:@"1"] forKey:k];
        else [[NSUserDefaults standardUserDefaults] setObject:v forKey:k];
        [[NSUserDefaults standardUserDefaults] synchronize];
    } else if ([cmd isEqual:@"sendui"]) {
        // Bảng «Gửi sách cho…» (chọn chat) của một cuốn — như bấm «Gửi cho bạn qua Telegram…» trong bảng «⋯»
        [GBBookActions send:@{@"key": arg, @"title": [[arg lastPathComponent] stringByDeletingPathExtension]} from:top];
    } else if ([cmd isEqual:@"theme"]) {
        [GMTheme setMode:arg.integerValue];
        [(GMAppDelegate *)[UIApplication sharedApplication].delegate reloadTheme];
    } else if ([cmd isEqual:@"tab"]) {
        GMRoot().selectedIndex = arg.integerValue;
    } else if ([cmd isEqual:@"select"]) {
        NSArray *n = [arg componentsSeparatedByString:@" "];
        UITableView *tv = [top respondsToSelector:@selector(tableView)] ? [(id)top tableView] : nil;
        if (!tv || n.count < 2) {
            GMLog(@"[thử] select: không có bảng");
            return;
        }
        NSIndexPath *ip = [NSIndexPath indexPathForRow:[[n objectAtIndex:1] integerValue] inSection:[[n objectAtIndex:0] integerValue]];
        if (ip.section >= tv.numberOfSections || ip.row >= [tv numberOfRowsInSection:ip.section]) {
            GMLog(@"[thử] select: bảng chưa có hàng %ld/%ld", (long)ip.section, (long)ip.row);
            return;
        }
        [tv selectRowAtIndexPath:ip animated:NO scrollPosition:UITableViewScrollPositionMiddle];
        [tv.delegate tableView:tv didSelectRowAtIndexPath:ip];
    } else if ([cmd isEqual:@"back"]) {
        [top.navigationController popViewControllerAnimated:YES];
    } else if ([cmd isEqual:@"dismiss"]) {
        [GMRoot() dismissViewControllerAnimated:YES completion:nil];
    } else if ([cmd isEqual:@"open"]) {
        [GMRoot() openBook:@{@"key": arg, @"title": [arg lastPathComponent]}];
    } else if ([cmd isEqual:@"openfile"]) {
        [GMRoot() openBook:@{@"key": [@"file:" stringByAppendingString:arg], @"title": [[arg lastPathComponent] stringByDeletingPathExtension]}];
    } else if ([cmd isEqual:@"r"]) {
        GBReaderViewController *r = GMRoot().reader;
        if (!r) {
            GMLog(@"[thử] r: chưa mở sách");
            return;
        }
        NSRange s2 = [arg rangeOfString:@" "];
        NSString *rc = s2.location == NSNotFound ? arg : [arg substringToIndex:s2.location];
        NSString *ra = s2.location == NSNotFound ? @"" : [arg substringFromIndex:s2.location + 1];
        GMLog(@"[thử] màn đọc: %@", [r debug:rc arg:ra]);
    } else if ([cmd isEqual:@"snap"]) {
        UIWindow *w = [UIApplication sharedApplication].keyWindow;
        UIGraphicsBeginImageContextWithOptions(w.bounds.size, YES, 0);
        [w.layer renderInContext:UIGraphicsGetCurrentContext()];
        UIImage *img = UIGraphicsGetImageFromCurrentImageContext();
        UIGraphicsEndImageContext();
        NSString *path = arg.length ? arg : @"/var/mobile/Documents/gbsnap.png";
        [UIImagePNGRepresentation(img) writeToFile:path atomically:YES];
        GMLog(@"[thử] snap → %@ (%.0fx%.0f)", path, img.size.width * img.scale, img.size.height * img.scale);
    } else if ([cmd isEqual:@"perform"]) {
        SEL sel = NSSelectorFromString(arg);
        if ([top respondsToSelector:sel]) {
#pragma clang diagnostic push
#pragma clang diagnostic ignored "-Warc-performSelector-leaks"
            [top performSelector:sel];
#pragma clang diagnostic pop
        } else {
            GMLog(@"[thử] perform: %@ không có %@", NSStringFromClass([top class]), arg);
        }
    } else if ([cmd isEqual:@"updateapi"]) {
        // updateapi <url> — nguồn bản mới để thử (rỗng = dùng Info.plist); updatecheck — kiểm ngay như chạm «Phiên bản»
        if (arg.length) [[NSUserDefaults standardUserDefaults] setObject:arg forKey:@"update.api"];
        else [[NSUserDefaults standardUserDefaults] removeObjectForKey:@"update.api"];
        [[NSUserDefaults standardUserDefaults] removeObjectForKey:@"update.snooze"];
        [[NSUserDefaults standardUserDefaults] synchronize];
    } else if ([cmd isEqual:@"updatecheck"]) {
        [GMUpdate checkManual];
    } else if ([cmd isEqual:@"otalocal"]) {
        // otalocal <tệp .deb> — cài gói + <tệp>.sig đã chép vào Documents/GramBook-update qua gbupdate (thử OTA không cần GitHub)
        [GMUpdate debugInstall:arg];
    } else if ([cmd isEqual:@"alertbtn"]) {
        // alertbtn <i> — bấm nút i của hộp thoại của app (UIAlertView của GramBook, không phải hộp hệ thống)
        for (UIWindow *w in [UIApplication sharedApplication].windows) {
            for (UIView *v in w.subviews) {
                if (![v isKindOfClass:[UIAlertView class]]) continue;
                UIAlertView *a = (UIAlertView *)v;
                GMLog(@"[thử] hộp «%@»: nút %ld «%@»", a.title, (long)arg.integerValue, [a buttonTitleAtIndex:arg.integerValue]);
                if ([a.delegate respondsToSelector:@selector(alertView:clickedButtonAtIndex:)]) [a.delegate alertView:a clickedButtonAtIndex:arg.integerValue];
                [a dismissWithClickedButtonIndex:arg.integerValue animated:YES];
            }
        }
    } else {
        GMLog(@"[thử] lệnh lạ: %@", cmd);
    }
}

@end
