#import "GBReadingViewController.h"
#import "GBBookActions.h"
#import "GBBookCell.h"
#import "GMCore.h"
#import "GMRootController.h"
#import "GMTheme.h"

@implementation GBReadingViewController {
    NSArray *_reading, *_done;
}

- (void)viewDidLoad {
    [super viewDidLoad];
    self.title = @"Đang đọc";
    self.tableView.rowHeight = [GBBookCell height];
    [[NSNotificationCenter defaultCenter] addObserver:self selector:@selector(load) name:@"GBReadingChanged" object:nil];
}

- (void)dealloc {
    [[NSNotificationCenter defaultCenter] removeObserver:self];
}

- (void)viewWillAppear:(BOOL)animated {
    [super viewWillAppear:animated];
    [self load];
}

- (void)load {
    [GMCore call:@"reading.recent" args:@{@"limit": @200} done:^(id ok, NSDictionary *err) {
        if (![ok isKindOfClass:[NSArray class]]) return;
        NSMutableArray *r = [NSMutableArray array], *d = [NSMutableArray array];
        for (NSDictionary *x in ok) [[GMVal(x, @"finished") boolValue] ? d : r addObject:[self bookFrom:x]];
        _reading = r;
        _done = d;
        [self.tableView reloadData];
        [self updateEmpty];
    }];
}

/// Bản ghi đọc → dict sách cho GBBookCell.
- (NSDictionary *)bookFrom:(NSDictionary *)x {
    NSMutableDictionary *b = [NSMutableDictionary dictionary];
    for (NSString *k in @[@"key", @"title", @"author", @"ext", @"progress", @"finished"]) {
        id v = GMVal(x, k);
        if (v) [b setObject:v forKey:k];
    }
    NSString *ext = GMVal(x, @"ext");
    [b setObject:@(GBReadableExt(ext)) forKey:@"ok"];
    NSString *chapter = GMVal(x, @"chapter");
    double p = [GMVal(x, @"progress") doubleValue];
    NSString *cap = chapter.length ? [NSString stringWithFormat:@"%@ · %.0f%%", chapter, p * 100] : [NSString stringWithFormat:@"%.0f%%", p * 100];
    [b setObject:cap forKey:@"caption"];
    return b;
}

- (void)updateEmpty {
    if (_reading.count || _done.count) {
        self.tableView.tableFooterView = [[UIView alloc] initWithFrame:CGRectZero];
        return;
    }
    UIView *f = [[UIView alloc] initWithFrame:CGRectMake(0, 0, self.tableView.bounds.size.width, 140)];
    UILabel *l = [[UILabel alloc] initWithFrame:CGRectMake(20, 20, f.bounds.size.width - 40, 110)];
    l.text = @"Chưa đọc cuốn nào.\nMở một cuốn ở «Thư viện» (sách từ Telegram) hoặc «Nguồn» → «Sách trên máy».";
    l.numberOfLines = 0;
    l.textAlignment = GMAlignCenter;
    l.font = [UIFont systemFontOfSize:14];
    l.textColor = [GMTheme secondaryText];
    l.backgroundColor = [UIColor clearColor];
    l.autoresizingMask = UIViewAutoresizingFlexibleWidth;
    [f addSubview:l];
    self.tableView.tableFooterView = f;
}

- (NSInteger)numberOfSectionsInTableView:(UITableView *)tv {
    return 2;
}

- (NSString *)tableView:(UITableView *)tv titleForHeaderInSection:(NSInteger)s {
    if (s == 0) return _reading.count ? @"Đang đọc" : nil;
    return _done.count ? @"Đã đọc xong" : nil;
}

/// Tiêu đề nhóm: dải kính theo giao diện (dải xám mặc định của iOS 6 chói khi tối).
- (UIView *)tableView:(UITableView *)tv viewForHeaderInSection:(NSInteger)s {
    return [GMTheme plainHeader:[self tableView:tv titleForHeaderInSection:s]];
}

- (CGFloat)tableView:(UITableView *)tv heightForHeaderInSection:(NSInteger)s {
    return [self tableView:tv titleForHeaderInSection:s].length ? GMPlainHeaderHeight : 0;
}

- (NSInteger)tableView:(UITableView *)tv numberOfRowsInSection:(NSInteger)s {
    return s == 0 ? _reading.count : _done.count;
}

- (NSDictionary *)bookAt:(NSIndexPath *)ip {
    NSArray *a = ip.section == 0 ? _reading : _done;
    return ip.row < (NSInteger)a.count ? [a objectAtIndex:ip.row] : nil;
}

- (UITableViewCell *)tableView:(UITableView *)tv cellForRowAtIndexPath:(NSIndexPath *)ip {
    GBBookCell *c = (GBBookCell *)[tv dequeueReusableCellWithIdentifier:@"book"];
    if (!c) c = [[GBBookCell alloc] initWithStyle:UITableViewCellStyleDefault reuseIdentifier:@"book"];
    NSDictionary *b = [self bookAt:ip];
    [c setBook:b progress:-1 showSource:NO];
    __weak GBReadingViewController *ws = self;
    __weak GBBookCell *wc = c;
    c.more = ^{
        [GBBookActions showFor:b in:ws from:wc collection:nil changed:^(NSDictionary *nb) { [ws load]; } removed:^{ [ws load]; }];
    };
    return c;
}

- (void)tableView:(UITableView *)tv didSelectRowAtIndexPath:(NSIndexPath *)ip {
    [tv deselectRowAtIndexPath:ip animated:YES];
    NSDictionary *b = [self bookAt:ip];
    if (b) [GMRoot() openBook:b];
}

- (NSString *)tableView:(UITableView *)tv titleForDeleteConfirmationButtonForRowAtIndexPath:(NSIndexPath *)ip {
    return @"Bỏ khỏi danh sách";
}

- (void)tableView:(UITableView *)tv commitEditingStyle:(UITableViewCellEditingStyle)style forRowAtIndexPath:(NSIndexPath *)ip {
    NSDictionary *b = [self bookAt:ip];
    if (style != UITableViewCellEditingStyleDelete || !b) return;
    [GMCore call:@"reading.forget" args:@{@"key": GMVal(b, @"key")} done:^(id ok, NSDictionary *err) { [self load]; }];
}

@end
