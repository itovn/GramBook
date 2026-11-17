#import "GBBookActions.h"
#import "GMCore.h"
#import "GMImages.h"
#import "GMRootController.h"
#import "GMTheme.h"
#import <QuartzCore/QuartzCore.h>

/// Các bảng đang mở (UIActionSheet / UIAlertView không giữ delegate).
static NSMutableSet *gOpen;

#pragma mark - Bảng thao tác dùng khối (chép từ GramMusic)

@interface GBSheet : NSObject <UIActionSheetDelegate, UIAlertViewDelegate>
@property (nonatomic, strong) NSMutableArray *actions;  // khối theo thứ tự nút (không gồm Huỷ)
@property (nonatomic, copy) void (^textDone)(NSString *text);
@property (nonatomic, copy) void (^confirmed)(void);
@end

@implementation GBSheet

- (void)actionSheet:(UIActionSheet *)sheet clickedButtonAtIndex:(NSInteger)i {
    if (i >= 0 && i < (NSInteger)_actions.count) ((void (^)(void))[_actions objectAtIndex:i])();
}

- (void)actionSheet:(UIActionSheet *)sheet didDismissWithButtonIndex:(NSInteger)i {
    [gOpen removeObject:self];
}

- (void)alertView:(UIAlertView *)alert clickedButtonAtIndex:(NSInteger)i {
    if (i == 1 && _confirmed) _confirmed();
    if (i == 1 && _textDone) {
        NSString *t = [[alert textFieldAtIndex:0].text stringByTrimmingCharactersInSet:[NSCharacterSet whitespaceCharacterSet]];
        if (t.length) _textDone(t);
    }
}

- (void)alertView:(UIAlertView *)alert didDismissWithButtonIndex:(NSInteger)i {
    [gOpen removeObject:self];
}

@end

void GMAskText(NSString *title, NSString *placeholder, NSString *initial, void (^done)(NSString *text)) {
    if (!gOpen) gOpen = [NSMutableSet set];
    GBSheet *h = [[GBSheet alloc] init];
    h.textDone = done;
    UIAlertView *a = [[UIAlertView alloc] initWithTitle:title message:nil delegate:h cancelButtonTitle:@"Huỷ" otherButtonTitles:@"Lưu", nil];
    a.alertViewStyle = UIAlertViewStylePlainTextInput;
    UITextField *f = [a textFieldAtIndex:0];
    f.placeholder = placeholder;
    f.text = initial;
    f.autocapitalizationType = UITextAutocapitalizationTypeSentences;
    f.keyboardAppearance = [GMTheme keyboard];
    [gOpen addObject:h];
    [a show];
}

/// Hỏi lại trước việc khó rút lại (gửi tin cho người khác…): Huỷ / `ok`.
static void GBConfirm(NSString *title, NSString *message, NSString *ok, void (^done)(void)) {
    if (!gOpen) gOpen = [NSMutableSet set];
    GBSheet *h = [[GBSheet alloc] init];
    h.confirmed = done;
    UIAlertView *a = [[UIAlertView alloc] initWithTitle:title message:message delegate:h cancelButtonTitle:@"Huỷ" otherButtonTitles:ok, nil];
    [gOpen addObject:h];
    [a show];
}

#pragma mark - Gửi sách cho bạn (chọn chat)

/// Danh sách chat (`chats.recent`: Tin nhắn đã lưu + chat gần đây); chọn một chat → hỏi lại → `book.forward` (chuyển tiếp tin gốc,
/// không tải lên lại).
@interface GBPickChatViewController : UITableViewController
@property (nonatomic, copy) NSDictionary *book;
@end

@implementation GBPickChatViewController {
    NSArray *_chats;
    UIActivityIndicatorView *_spin;
    BOOL _sending;
}

- (void)viewDidLoad {
    [super viewDidLoad];
    self.title = @"Gửi sách cho…";
    self.tableView.rowHeight = 50;
    self.navigationItem.leftBarButtonItem =
        [[UIBarButtonItem alloc] initWithTitle:@"Huỷ" style:UIBarButtonItemStyleBordered target:self action:@selector(close)];
    _spin = [[UIActivityIndicatorView alloc] initWithActivityIndicatorStyle:UIActivityIndicatorViewStyleWhite];
    _spin.hidesWhenStopped = YES;
    self.navigationItem.rightBarButtonItem = [[UIBarButtonItem alloc] initWithCustomView:_spin];
    [_spin startAnimating];
    [GMCore call:@"chats.recent" args:@{@"limit": @60} done:^(id ok, NSDictionary *err) {
        [_spin stopAnimating];
        if (err) {
            [GMTheme toast:GMErrorText(err) in:self.view];
            return;
        }
        _chats = [ok isKindOfClass:[NSArray class]] ? ok : @[];
        [self.tableView reloadData];
    }];
}

- (void)close {
    [self dismissViewControllerAnimated:YES completion:nil];
}

- (NSInteger)tableView:(UITableView *)tv numberOfRowsInSection:(NSInteger)s {
    return _chats.count;
}

- (UITableViewCell *)tableView:(UITableView *)tv cellForRowAtIndexPath:(NSIndexPath *)ip {
    UITableViewCell *c = [tv dequeueReusableCellWithIdentifier:@"chat"] ?:
        [[UITableViewCell alloc] initWithStyle:UITableViewCellStyleSubtitle reuseIdentifier:@"chat"];
    NSDictionary *d = [_chats objectAtIndex:ip.row];
    NSString *peer = GMVal(d, @"peer"), *kind = GMVal(d, @"kind"), *name = GMVal(d, @"name"), *user = GMVal(d, @"username");
    c.textLabel.text = name;
    c.textLabel.font = [UIFont boldSystemFontOfSize:15];
    if ([kind isEqual:@"self"]) c.detailTextLabel.text = @"Gửi cho chính mình";
    else if ([kind isEqual:@"user"]) c.detailTextLabel.text = user.length ? [@"@" stringByAppendingString:user] : @"Người dùng";
    else if ([kind isEqual:@"bot"]) c.detailTextLabel.text = user.length ? [NSString stringWithFormat:@"Bot · @%@", user] : @"Bot";
    else if ([kind isEqual:@"channel"]) c.detailTextLabel.text = @"Kênh (bạn quản trị)";
    else c.detailTextLabel.text = @"Nhóm";
    c.imageView.image = [kind isEqual:@"self"] ? [GMTheme settingsIcon:@"saved"] : [GMTheme avatarFor:peer name:name size:32];
    NSString *photo = GMVal(d, @"photo");
    if (photo && ![kind isEqual:@"self"]) {
        __weak UITableViewCell *wc = c;
        [GMImages avatarForPeer:peer photo:photo dc:GMVal(d, @"photo_dc") size:32 done:^(UIImage *img) {
            NSIndexPath *now = [tv indexPathForCell:wc];
            if (now && now.row < (NSInteger)_chats.count && [GMVal([_chats objectAtIndex:now.row], @"peer") isEqual:peer]) {
                wc.imageView.image = img;
            }
        }];
    }
    return c;
}

- (void)tableView:(UITableView *)tv didSelectRowAtIndexPath:(NSIndexPath *)ip {
    [tv deselectRowAtIndexPath:ip animated:YES];
    if (_sending) return;
    NSDictionary *d = [_chats objectAtIndex:ip.row];
    NSString *name = GMVal(d, @"name") ?: @"";
    NSString *title = GMVal(_book, @"title") ?: @"";
    __weak GBPickChatViewController *ws = self;
    GBConfirm(@"Gửi sách", [NSString stringWithFormat:@"Gửi «%@» cho «%@»?", title, name], @"Gửi", ^{ [ws send:d]; });
}

- (void)send:(NSDictionary *)d {
    _sending = YES;
    [_spin startAnimating];
    UIView *host = self.presentingViewController.view;
    NSString *name = GMVal(d, @"name") ?: @"";
    [GMCore call:@"book.forward" args:@{@"key": GMVal(_book, @"key") ?: @"", @"to": GMVal(d, @"peer") ?: @""}
            done:^(id ok, NSDictionary *err) {
                _sending = NO;
                [_spin stopAnimating];
                if (err) {
                    [GMTheme toast:GMErrorText(err) in:self.view];
                    return;
                }
                [self close];
                [GMTheme toast:[NSString stringWithFormat:@"Đã gửi cho «%@»", name] in:host];
            }];
}

@end

#pragma mark - Chọn bộ sưu tập

@interface GBPickCollectionViewController : UITableViewController
@property (nonatomic, copy) NSArray *keys;
@end

@implementation GBPickCollectionViewController {
    NSArray *_lists;
}

- (void)viewDidLoad {
    [super viewDidLoad];
    self.title = @"Thêm vào bộ sưu tập";
    self.navigationItem.leftBarButtonItem =
        [[UIBarButtonItem alloc] initWithTitle:@"Huỷ" style:UIBarButtonItemStyleBordered target:self action:@selector(close)];
    [GMCore call:@"col.list" args:nil done:^(id ok, NSDictionary *err) {
        _lists = GMVal(ok, @"lists") ?: @[];
        [self.tableView reloadData];
    }];
}

- (void)close {
    [self dismissViewControllerAnimated:YES completion:nil];
}

- (NSInteger)tableView:(UITableView *)tv numberOfRowsInSection:(NSInteger)s {
    return _lists.count + 1;
}

- (UITableViewCell *)tableView:(UITableView *)tv cellForRowAtIndexPath:(NSIndexPath *)ip {
    UITableViewCell *c = [tv dequeueReusableCellWithIdentifier:@"p"] ?:
        [[UITableViewCell alloc] initWithStyle:UITableViewCellStyleSubtitle reuseIdentifier:@"p"];
    if (ip.row == 0) {
        c.textLabel.text = @"+ Bộ sưu tập mới…";
        c.textLabel.textColor = [GMTheme accent];
        c.detailTextLabel.text = nil;
    } else {
        NSDictionary *p = [_lists objectAtIndex:ip.row - 1];
        c.textLabel.text = GMVal(p, @"name");
        c.textLabel.textColor = [GMTheme text];
        c.detailTextLabel.text = [NSString stringWithFormat:@"%@ cuốn", GMVal(p, @"count")];
    }
    return c;
}

- (void)tableView:(UITableView *)tv didSelectRowAtIndexPath:(NSIndexPath *)ip {
    [tv deselectRowAtIndexPath:ip animated:YES];
    UIView *host = self.presentingViewController.view;
    NSArray *keys = _keys;
    if (ip.row == 0) {
        GMAskText(@"Bộ sưu tập mới", @"Tên bộ sưu tập", nil, ^(NSString *name) {
            [GMCore call:@"col.create" args:@{@"name": name, @"keys": keys} done:^(id ok, NSDictionary *err) {
                [GMTheme toast:err ? GMErrorText(err) : [NSString stringWithFormat:@"Đã tạo «%@»", name] in:host];
            }];
            [self close];
        });
        return;
    }
    NSDictionary *p = [_lists objectAtIndex:ip.row - 1];
    NSString *name = GMVal(p, @"name");
    [GMCore call:@"col.add" args:@{@"id": GMVal(p, @"id"), @"keys": keys} done:^(id ok, NSDictionary *err) {
        [GMTheme toast:err ? GMErrorText(err) : [NSString stringWithFormat:@"Đã thêm vào «%@»", name] in:host];
    }];
    [self close];
}

@end

#pragma mark - Bảng thao tác của một cuốn

@implementation GBBookActions

+ (void)send:(NSDictionary *)book from:(UIViewController *)vc {
    GBPickChatViewController *p = [[GBPickChatViewController alloc] initWithStyle:UITableViewStylePlain];
    p.book = book;
    UINavigationController *n = [[UINavigationController alloc] initWithRootViewController:p];
    n.modalPresentationStyle = UIModalPresentationFormSheet;
    [vc presentViewController:n animated:YES completion:nil];
}

+ (void)addKeys:(NSArray *)keys toCollectionFrom:(UIViewController *)vc {
    if (!keys.count) return;
    GBPickCollectionViewController *p = [[GBPickCollectionViewController alloc] initWithStyle:UITableViewStylePlain];
    p.keys = keys;
    UINavigationController *n = [[UINavigationController alloc] initWithRootViewController:p];
    n.modalPresentationStyle = UIModalPresentationFormSheet;
    [vc presentViewController:n animated:YES completion:nil];
}

+ (void)showFor:(NSDictionary *)book in:(UIViewController *)vc from:(UIView *)anchor collection:(id)collectionId
        changed:(void (^)(NSDictionary *))changed removed:(void (^)(void))removed {
    if (!gOpen) gOpen = [NSMutableSet set];
    NSString *key = GMVal(book, @"key");
    if (!key) return;
    GBSheet *h = [[GBSheet alloc] init];
    h.actions = [NSMutableArray array];
    UIActionSheet *a = [[UIActionSheet alloc] initWithTitle:GMVal(book, @"title") delegate:h cancelButtonTitle:nil
                                     destructiveButtonTitle:nil otherButtonTitles:nil];
    UIView *host = vc.view;
    void (^add)(NSString *, void (^)(void)) = ^(NSString *t, void (^blk)(void)) {
        [a addButtonWithTitle:t];
        [h.actions addObject:[blk copy]];
    };
    BOOL missing = [GMVal(book, @"missing") boolValue];
    BOOL local = [key hasPrefix:@"file:"];
    if (!missing) {
        add([GMVal(book, @"ok") boolValue] ? @"Đọc" : @"Đọc (thử)", ^{ [GMRoot() openBook:book]; });
    }
    NSString *dl = GMVal(book, @"dl");
    if (!missing && !local) {
        if ([dl isEqual:@"offline"] || [dl isEqual:@"queued"]) {
            add(@"Xoá bản tải về máy", ^{
                [GMCore call:@"dl.remove" args:@{@"keys": @[key]} done:^(id ok, NSDictionary *err) {
                    if (err) {
                        [GMTheme toast:GMErrorText(err) in:host];
                        return;
                    }
                    [GMTheme toast:[NSString stringWithFormat:@"Đã xoá (%@)", [GMTheme sizeText:[GMVal(ok, @"freed") doubleValue]]] in:host];
                    NSMutableDictionary *m = [book mutableCopy];
                    [m removeObjectForKey:@"dl"];
                    if (changed) changed(m);
                    if ([collectionId isEqual:@"offline"] && removed) removed();
                }];
            });
        } else {
            add(@"Tải về máy (đọc khi không có mạng)", ^{
                [GMCore call:@"dl.add" args:@{@"keys": @[key]} done:^(id ok, NSDictionary *err) {
                    if (err) {
                        [GMTheme toast:GMErrorText(err) in:host];
                        return;
                    }
                    [GMTheme toast:@"Đang tải về máy…" in:host];
                    NSMutableDictionary *m = [book mutableCopy];
                    [m setObject:@"queued" forKey:@"dl"];
                    if (changed) changed(m);
                }];
            });
        }
    }
    BOOL fav = [GMVal(book, @"fav") boolValue];
    add(fav ? @"Bỏ Yêu thích" : @"Yêu thích", ^{
        [GMCore call:@"fav.set" args:@{@"key": key, @"on": @(!fav)} done:^(id ok, NSDictionary *err) {
            if (err) {
                [GMTheme toast:GMErrorText(err) in:host];
                return;
            }
            NSMutableDictionary *m = [book mutableCopy];
            [m setObject:@(!fav) forKey:@"fav"];
            if (changed) changed(m);
            if (fav && [collectionId isEqual:@"fav"] && removed) removed();
        }];
    });
    BOOL want = [GMVal(book, @"want") boolValue];
    add(want ? @"Bỏ khỏi «Muốn đọc»" : @"Muốn đọc", ^{
        [GMCore call:@"want.set" args:@{@"key": key, @"on": @(!want)} done:^(id ok, NSDictionary *err) {
            if (err) {
                [GMTheme toast:GMErrorText(err) in:host];
                return;
            }
            NSMutableDictionary *m = [book mutableCopy];
            [m setObject:@(!want) forKey:@"want"];
            if (changed) changed(m);
            if (want && [collectionId isEqual:@"want"] && removed) removed();
        }];
    });
    if (!local) {
        add(@"Thêm vào bộ sưu tập…", ^{ [self addKeys:@[key] toCollectionFrom:vc]; });
    }
    // «Hạn chế lưu nội dung» (nf): Telegram cấm chuyển tiếp — không hiện nút gửi. Sách trên máy: lõi tải tệp lên rồi gửi.
    if (!missing && ![GMVal(book, @"nf") boolValue]) {
        add(@"Gửi cho bạn qua Telegram…", ^{ [self send:book from:vc]; });
    }
    BOOL finished = [GMVal(book, @"finished") boolValue];
    add(finished ? @"Đánh dấu chưa đọc xong" : @"Đánh dấu đã đọc xong", ^{
        [GMCore call:@"reading.finish" args:@{@"key": key, @"on": @(!finished)} done:^(id ok, NSDictionary *err) {
            if (err) {
                [GMTheme toast:GMErrorText(err) in:host];
                return;
            }
            NSMutableDictionary *m = [book mutableCopy];
            [m setObject:@(!finished) forKey:@"finished"];
            if (changed) changed(m);
        }];
    });
    if ([collectionId isKindOfClass:[NSNumber class]] || [collectionId isEqual:@"recent"]) {
        add(@"Bỏ khỏi danh sách này", ^{
            [GMCore call:@"col.remove" args:@{@"id": collectionId, @"keys": @[key]} done:^(id ok, NSDictionary *err) {
                if (err) [GMTheme toast:GMErrorText(err) in:host];
                else if (removed) removed();
            }];
        });
    }
    a.cancelButtonIndex = [a addButtonWithTitle:@"Huỷ"];
    [gOpen addObject:h];
    if (UI_USER_INTERFACE_IDIOM() == UIUserInterfaceIdiomPad && anchor) {
        [a showFromRect:anchor.bounds inView:anchor animated:YES];
    } else {
        [a showInView:vc.view.window ?: vc.view];
    }
}

@end
