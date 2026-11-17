#import "GBBooksViewController.h"
#import "GBBookActions.h"
#import "GBBookCell.h"
#import "GBBookList.h"
#import "GMArt.h"
#import "GMCompat.h"
#import "GMCore.h"
#import "GMRootController.h"
#import "GMSourcesViewController.h"
#import "GMTheme.h"
#import <QuartzCore/QuartzCore.h>

/// «1.234» (dấu chấm ngăn nghìn).
static NSString *GBCount(NSInteger n) {
    if (n < 1000) return [NSString stringWithFormat:@"%ld", (long)n];
    return [NSString stringWithFormat:@"%@.%03ld", GBCount(n / 1000), (long)(n % 1000)];
}

/// Bộ lọc: (tên hiện, filter, fmt).
static NSArray *GBFilters(void) {
    return @[
        @[@"Tất cả", @"all", @""], @[@"Đọc được", @"readable", @""], @[@"EPUB", @"all", @"epub"], @[@"Kindle (MOBI / AZW3)", @"all", @"kindle"],
        @[@"PDF", @"all", @"pdf"], @[@"Truyện chữ / tài liệu", @"all", @"text"], @[@"Đã tải về máy", @"offline", @""],
        @[@"Đang đọc dở", @"reading", @""], @[@"Đã đọc xong", @"done", @""], @[@"Chưa đọc", @"unread", @""],
    ];
}

@implementation GBBooksViewController {
    GBBookList *_list;
    NSArray *_items;   // bộ sưu tập: cả danh sách
    NSArray *_shown;   // bộ sưu tập sau khi lọc tìm
    UISearchBar *_search;
    UISegmentedControl *_sortCtl;
    UIButton *_filterBtn;
    UILabel *_status;
    NSMutableDictionary *_progress;
    NSTimer *_searchTimer, *_reloadTimer;
    NSDictionary *_sync;
    id _refresh;  // UIRefreshControl (iOS 6)
    NSMutableArray *_menu;
    NSInteger _filterIndex;
}

- (instancetype)initWithKind:(GBBooksKind)kind {
    if ((self = [super initWithStyle:UITableViewStylePlain])) {
        _kind = kind;
        _progress = [NSMutableDictionary dictionary];
        if (kind != GBBooksCollection) {
            _list = [[GBBookList alloc] init];
            NSString *saved = [[NSUserDefaults standardUserDefaults] stringForKey:@"books.sort"];
            _list.sort = kind == GBBooksOffline ? @"title" : (saved ?: @"date");
            _list.filter = kind == GBBooksOffline ? @"offline" : @"all";
        }
        if (kind == GBBooksLibrary) self.title = @"Thư viện";
        else if (kind == GBBooksOffline) self.title = @"Đã tải về máy";
    }
    return self;
}

- (void)dealloc {
    [[NSNotificationCenter defaultCenter] removeObserver:self];
    [_searchTimer invalidate];
    [_reloadTimer invalidate];
}

- (void)viewDidLoad {
    [super viewDidLoad];
    self.tableView.rowHeight = [GBBookCell height];
    if (_kind == GBBooksSource) {
        _list.source = _source;
        _list.topic = _topic;
    }
    __weak GBBooksViewController *ws = self;
    _list.changed = ^{ [ws listChanged]; };
    [self buildHeader];
    self.navigationItem.rightBarButtonItem =
        [GMTheme barButtonImage:[GMArt glyph:@"more" size:20 color:[GMTheme barText]] target:self action:@selector(showMenu)];
    if (_kind == GBBooksLibrary || _kind == GBBooksSource) {
        Class rc = NSClassFromString(@"UIRefreshControl");
        if (rc) {
            GM_IOS6_BEGIN
            UIRefreshControl *r = [[rc alloc] init];
            [r addTarget:self action:@selector(refresh) forControlEvents:UIControlEventValueChanged];
            self.refreshControl = r;
            _refresh = r;
            GM_IOS6_END
        }
    }
    [[NSNotificationCenter defaultCenter] addObserver:self selector:@selector(coreEvent:) name:GMCoreEventNotification object:nil];
    if (_list) [_list reload];
    if (_kind == GBBooksLibrary) [self loadStats];
}

- (void)viewWillAppear:(BOOL)animated {
    [super viewWillAppear:animated];
    if (_kind == GBBooksCollection) [self loadItems];
    else if (_list.loaded) [_list reload];  // vừa đọc / tải / xoá ở màn khác: tiến độ, dấu đã tải
}

- (void)loggedIn {
    [_list reload];
    [self loadStats];
}

#pragma mark - Đầu bảng

- (void)buildHeader {
    CGFloat w = self.tableView.bounds.size.width;
    UIView *h = [[UIView alloc] initWithFrame:CGRectMake(0, 0, w, _list ? 106 : 66)];
    h.backgroundColor = [UIColor clearColor];
    _search = [[UISearchBar alloc] initWithFrame:CGRectMake(0, 0, w, 44)];
    _search.placeholder = _list ? @"Tìm tên sách, tác giả (không cần dấu)" : @"Tìm trong danh sách";
    _search.delegate = self;
    _search.autoresizingMask = UIViewAutoresizingFlexibleWidth;
    // UISearchBar của iOS 5 / 6 không có setKeyboardAppearance: (dựng được, chạy thì crash) → đặt cho ô nhập bên trong
    for (UIView *v in _search.subviews) {
        if ([v isKindOfClass:[UITextField class]]) ((UITextField *)v).keyboardAppearance = [GMTheme keyboard];
    }
    [h addSubview:_search];
    if (_list) {
        _sortCtl = [[UISegmentedControl alloc] initWithItems:@[@"Mới", @"A–Z", @"Tác giả"]];
        _sortCtl.segmentedControlStyle = UISegmentedControlStyleBar;
        _sortCtl.tintColor = [GMTheme accent];
        _sortCtl.frame = CGRectMake(w - 8 - 150, 50, 150, 30);
        _sortCtl.autoresizingMask = UIViewAutoresizingFlexibleLeftMargin;
        _sortCtl.selectedSegmentIndex = [_list.sort isEqual:@"title"] ? 1 : [_list.sort isEqual:@"author"] ? 2 : 0;
        [_sortCtl addTarget:self action:@selector(sortChanged) forControlEvents:UIControlEventValueChanged];
        [h addSubview:_sortCtl];
        if (_kind != GBBooksOffline) {
            _filterBtn = [UIButton buttonWithType:UIButtonTypeCustom];
            _filterBtn.frame = CGRectMake(8, 50, MIN(200, w - 8 - 150 - 16), 30);
            _filterBtn.titleLabel.font = [UIFont boldSystemFontOfSize:13];
            [_filterBtn setTitleColor:[GMTheme accent] forState:UIControlStateNormal];
            [_filterBtn setTitleColor:[[GMTheme accent] colorWithAlphaComponent:0.4] forState:UIControlStateHighlighted];
            _filterBtn.layer.cornerRadius = 6;
            _filterBtn.layer.borderWidth = 1;
            _filterBtn.layer.borderColor = [[GMTheme accent] colorWithAlphaComponent:0.6].CGColor;
            _filterBtn.backgroundColor = [GMTheme glassFill];
            [_filterBtn addTarget:self action:@selector(chooseFilter) forControlEvents:UIControlEventTouchUpInside];
            [h addSubview:_filterBtn];
            [self updateFilterTitle];
        }
    }
    _status = [[UILabel alloc] initWithFrame:CGRectMake(10, _list ? 85 : 46, w - 20, 18)];
    _status.font = [UIFont systemFontOfSize:12];
    _status.textColor = [GMTheme secondaryText];
    _status.backgroundColor = [UIColor clearColor];
    _status.autoresizingMask = UIViewAutoresizingFlexibleWidth;
    [h addSubview:_status];
    self.tableView.tableHeaderView = h;
    [self updateStatus];
}

- (void)updateFilterTitle {
    NSArray *f = [GBFilters() objectAtIndex:_filterIndex];
    [_filterBtn setTitle:[NSString stringWithFormat:@"%@ ▾", [f objectAtIndex:0]] forState:UIControlStateNormal];
}

- (void)updateStatus {
    NSInteger n = _list ? _list.total : (NSInteger)_shown.count;
    NSString *count = [NSString stringWithFormat:@"%@ cuốn", GBCount(n)];
    BOOL running = [GMVal(_sync, @"running") boolValue];
    if (_kind == GBBooksLibrary && running) {
        NSInteger wait = [GMVal(_sync, @"wait") integerValue];
        NSInteger found = [GMVal(_sync, @"found") integerValue];
        if (wait > 0) _status.text = [NSString stringWithFormat:@"%@ · Telegram bảo chờ %ld giây rồi quét tiếp…", count, (long)wait];
        else {
            NSInteger ci = [GMVal(_sync, @"chat") integerValue], cn = [GMVal(_sync, @"chats") integerValue];
            NSString *where = cn > 0 ? [NSString stringWithFormat:@"chat %ld/%ld, ", (long)ci, (long)cn] : @"";
            _status.text = [NSString stringWithFormat:@"%@ · đang quét Telegram (%@%@ cuốn mới)…", count, where, GBCount(found)];
        }
    } else if (_kind == GBBooksLibrary && ![GMRoot().me count] && !_list.total) {
        _status.text = @"Chưa đăng nhập Telegram — vẫn đọc được sách trên máy (mục Nguồn → Sách trên máy).";
    } else {
        _status.text = count;
    }
    [self updateEmpty];
}

- (void)updateEmpty {
    BOOL empty = _list ? (_list.loaded && _list.total == 0) : (_items && !_shown.count);
    if (!empty) {
        if (self.tableView.tableFooterView.tag == 77) self.tableView.tableFooterView = [[UIView alloc] initWithFrame:CGRectZero];
        return;
    }
    NSString *text;
    if (_search.text.length > 0 || _filterIndex > 0) text = @"Không có sách nào khớp.";
    else if (_kind == GBBooksOffline) text = @"Chưa tải cuốn nào về máy.\nChạm «⋯» cạnh một cuốn → «Tải về máy» để đọc khi không có mạng.";
    else if (_kind == GBBooksCollection) text = @"Danh sách trống.\nChạm «⋯» cạnh một cuốn để thêm.";
    else if (_kind == GBBooksSource) text = @"Chưa có sách nào từ nguồn này.\nKéo xuống / chạm «⋯» → «Lấy sách mới».";
    else if ([GMVal(_sync, @"running") boolValue]) text = @"Đang tìm sách trong tài khoản Telegram…\nSách sẽ hiện dần ở đây.";
    else text = @"Chưa có sách nào.\nEPUB, MOBI, AZW3, PDF… gửi trong Tin nhắn đã lưu, nhóm, kênh của bạn sẽ hiện ở đây sau khi quét. Thêm "
                @"kênh sách công khai ở mục «Nguồn».";
    UIView *f = [[UIView alloc] initWithFrame:CGRectMake(0, 0, self.tableView.bounds.size.width, 140)];
    f.tag = 77;
    UILabel *l = [[UILabel alloc] initWithFrame:CGRectMake(20, 20, f.bounds.size.width - 40, 110)];
    l.text = text;
    l.numberOfLines = 0;
    l.textAlignment = GMAlignCenter;
    l.font = [UIFont systemFontOfSize:14];
    l.textColor = [GMTheme secondaryText];
    l.backgroundColor = [UIColor clearColor];
    l.autoresizingMask = UIViewAutoresizingFlexibleWidth;
    [f addSubview:l];
    self.tableView.tableFooterView = f;
}

#pragma mark - Dữ liệu

- (void)loadStats {
    [GMCore call:@"library.stats" args:nil done:^(id ok, NSDictionary *err) {
        if (err) return;
        NSDictionary *sync = GMVal(ok, @"sync");
        if ([sync isKindOfClass:[NSDictionary class]]) _sync = sync;
        [self updateStatus];
    }];
}

- (void)loadItems {
    if (!_collectionId) return;
    [GMCore call:@"col.items" args:@{@"id": _collectionId} done:^(id ok, NSDictionary *err) {
        if (err) {
            [GMTheme toast:GMErrorText(err) in:self.view];
            return;
        }
        _items = [ok isKindOfClass:[NSArray class]] ? ok : @[];
        [self filterItems];
    }];
}

- (void)filterItems {
    NSString *q = [_search.text stringByTrimmingCharactersInSet:[NSCharacterSet whitespaceCharacterSet]];
    if (!q.length) {
        _shown = _items;
    } else {
        NSMutableArray *v = [NSMutableArray array];
        NSStringCompareOptions o = NSCaseInsensitiveSearch | NSDiacriticInsensitiveSearch;
        for (NSDictionary *s in _items) {
            NSString *hay = [NSString stringWithFormat:@"%@ %@", GMVal(s, @"title") ?: @"", GMVal(s, @"author") ?: @""];
            if ([hay rangeOfString:q options:o].location != NSNotFound) [v addObject:s];
        }
        _shown = v;
    }
    [self.tableView reloadData];
    [self updateStatus];
}

- (void)listChanged {
    [self.tableView reloadData];
    [self updateStatus];
    GM_IOS6_BEGIN
    if (_refresh) [(UIRefreshControl *)_refresh endRefreshing];
    GM_IOS6_END
}

/// Có mục chữ cái (theo dữ liệu đã tải — lõi trả `sections` rỗng khi xếp theo ngày).
- (BOOL)alpha {
    return _list && _list.sections.count > 1;
}

- (NSInteger)globalIndex:(NSIndexPath *)ip {
    if ([self alpha]) return [GMVal([_list.sections objectAtIndex:ip.section], @"i") integerValue] + ip.row;
    return ip.row;
}

- (NSDictionary *)bookAt:(NSInteger)g {
    if (_list) return [_list bookAt:g];
    return g >= 0 && g < (NSInteger)_shown.count ? [_shown objectAtIndex:g] : nil;
}

#pragma mark - Bảng

- (NSInteger)numberOfSectionsInTableView:(UITableView *)tv {
    return [self alpha] ? _list.sections.count : 1;
}

- (NSInteger)tableView:(UITableView *)tv numberOfRowsInSection:(NSInteger)s {
    if ([self alpha]) return [GMVal([_list.sections objectAtIndex:s], @"n") integerValue];
    return _list ? _list.total : _shown.count;
}

- (NSString *)tableView:(UITableView *)tv titleForHeaderInSection:(NSInteger)s {
    return [self alpha] ? GMVal([_list.sections objectAtIndex:s], @"t") : nil;
}

/// Tiêu đề nhóm: dải kính theo giao diện (dải xám mặc định của iOS 6 chói khi tối).
- (UIView *)tableView:(UITableView *)tv viewForHeaderInSection:(NSInteger)s {
    return [GMTheme plainHeader:[self tableView:tv titleForHeaderInSection:s]];
}

- (CGFloat)tableView:(UITableView *)tv heightForHeaderInSection:(NSInteger)s {
    return [self tableView:tv titleForHeaderInSection:s].length ? GMPlainHeaderHeight : 0;
}

- (NSArray *)sectionIndexTitlesForTableView:(UITableView *)tv {
    if (![self alpha] || _list.total < 30) return nil;
    if ([tv respondsToSelector:@selector(setSectionIndexColor:)]) {
        GM_IOS6_BEGIN
        tv.sectionIndexColor = [GMTheme accent];
        tv.sectionIndexTrackingBackgroundColor = [[GMTheme text] colorWithAlphaComponent:0.12];
        GM_IOS6_END
    }
    NSMutableArray *t = [NSMutableArray array];
    for (NSDictionary *s in _list.sections) [t addObject:GMVal(s, @"t") ?: @"#"];
    return t;
}

- (NSInteger)tableView:(UITableView *)tv sectionForSectionIndexTitle:(NSString *)title atIndex:(NSInteger)i {
    return i;
}

- (UITableViewCell *)tableView:(UITableView *)tv cellForRowAtIndexPath:(NSIndexPath *)ip {
    GBBookCell *c = (GBBookCell *)[tv dequeueReusableCellWithIdentifier:@"book"];
    if (!c) c = [[GBBookCell alloc] initWithStyle:UITableViewCellStyleDefault reuseIdentifier:@"book"];
    NSInteger g = [self globalIndex:ip];
    NSDictionary *b = [self bookAt:g];
    if (!b) {
        [c setLoading];
        return c;
    }
    NSNumber *p = [_progress objectForKey:GMVal(b, @"key") ?: @""];
    [c setBook:b progress:p ? p.doubleValue : -1 showSource:_kind != GBBooksSource];
    __weak GBBooksViewController *ws = self;
    __weak GBBookCell *wc = c;
    c.more = ^{ [ws actionsFor:g anchor:wc]; };
    return c;
}

- (void)actionsFor:(NSInteger)g anchor:(UIView *)anchor {
    NSDictionary *b = [self bookAt:g];
    if (!b) return;
    id col = _kind == GBBooksCollection ? _collectionId : (_kind == GBBooksOffline ? @"offline" : nil);
    __weak GBBooksViewController *ws = self;
    [GBBookActions showFor:b in:self from:anchor collection:col
        changed:^(NSDictionary *nb) { [ws bookChanged:nb]; }
        removed:^{ [ws bookRemoved:GMVal(b, @"key")]; }];
}

- (void)bookChanged:(NSDictionary *)b {
    if (_list) {
        [_list replaceBook:b];
        [self reloadVisible];
        return;
    }
    NSMutableArray *v = [_items mutableCopy];
    for (NSUInteger i = 0; i < v.count; i++) {
        if ([GMVal([v objectAtIndex:i], @"key") isEqual:GMVal(b, @"key")]) [v replaceObjectAtIndex:i withObject:b];
    }
    _items = v;
    [self filterItems];
}

- (void)bookRemoved:(NSString *)key {
    if (_list) {
        [_list reload];
        return;
    }
    NSMutableArray *v = [_items mutableCopy];
    for (NSInteger i = (NSInteger)v.count - 1; i >= 0; i--) {
        if ([GMVal([v objectAtIndex:i], @"key") isEqual:key]) [v removeObjectAtIndex:i];
    }
    _items = v;
    [self filterItems];
}

- (void)reloadVisible {
    NSArray *vis = [self.tableView indexPathsForVisibleRows];
    if (vis.count) [self.tableView reloadRowsAtIndexPaths:vis withRowAnimation:UITableViewRowAnimationNone];
}

- (void)tableView:(UITableView *)tv didSelectRowAtIndexPath:(NSIndexPath *)ip {
    [tv deselectRowAtIndexPath:ip animated:YES];
    [_search resignFirstResponder];
    NSDictionary *b = [self bookAt:[self globalIndex:ip]];
    if (!b) return;
    if ([GMVal(b, @"missing") boolValue]) {
        [GMTheme toast:@"Sách không còn trong thư viện (nguồn đã gỡ / tin đã xoá)." in:self.view];
        return;
    }
    if (![GMVal(b, @"ok") boolValue]) {
        [GMTheme toast:[NSString stringWithFormat:@"Chưa đọc được định dạng %@ — bản sau sẽ có.", [GMVal(b, @"ext") uppercaseString]] in:self.view];
        return;
    }
    [GMRoot() openBook:b];
}

#pragma mark - Menu, lọc, tìm, xếp, làm mới

- (void)sheet:(NSString *)title items:(NSArray *)items from:(id)anchor {
    UIActionSheet *a = [[UIActionSheet alloc] initWithTitle:title delegate:self cancelButtonTitle:nil destructiveButtonTitle:nil
                                          otherButtonTitles:nil];
    _menu = [NSMutableArray array];
    for (NSArray *it in items) {
        [a addButtonWithTitle:[it objectAtIndex:0]];
        [_menu addObject:[it objectAtIndex:1]];
    }
    a.cancelButtonIndex = [a addButtonWithTitle:@"Huỷ"];
    BOOL pad = UI_USER_INTERFACE_IDIOM() == UIUserInterfaceIdiomPad;
    if (pad && [anchor isKindOfClass:[UIBarButtonItem class]]) [a showFromBarButtonItem:anchor animated:YES];
    else if (pad && [anchor isKindOfClass:[UIView class]]) [a showFromRect:[anchor bounds] inView:anchor animated:YES];
    else [a showInView:self.view.window ?: self.view];
}

- (void)actionSheet:(UIActionSheet *)sheet clickedButtonAtIndex:(NSInteger)i {
    if (i >= 0 && i < (NSInteger)_menu.count) ((void (^)(void))[_menu objectAtIndex:i])();
}

- (void)showMenu {
    __weak GBBooksViewController *ws = self;
    NSMutableArray *items = [NSMutableArray array];
    if (_kind == GBBooksLibrary || _kind == GBBooksSource) [items addObject:@[@"Lấy sách mới", [^{ [ws refresh]; } copy]]];
    if (_forum && !_topic) [items addObject:@[@"Xem theo chủ đề", [^{ [ws openTopics]; } copy]]];
    if (_kind == GBBooksCollection && [_collectionId isEqual:@"recent"] && _items.count) {
        [items addObject:@[@"Xoá danh sách mở gần đây", [^{
            [GMCore call:@"col.remove" args:@{@"id": @"recent", @"keys": @[]} done:^(id ok, NSDictionary *err) { [ws loadItems]; }];
        } copy]]];
    }
    if (!items.count) return;
    [self sheet:nil items:items from:self.navigationItem.rightBarButtonItem];
}

- (void)chooseFilter {
    __weak GBBooksViewController *ws = self;
    NSMutableArray *items = [NSMutableArray array];
    NSArray *fs = GBFilters();
    for (NSUInteger i = 0; i < fs.count; i++) {
        NSString *t = [[fs objectAtIndex:i] objectAtIndex:0];
        if ((NSInteger)i == _filterIndex) t = [@"✓ " stringByAppendingString:t];
        [items addObject:@[t, [^{ [ws setFilterIndex:i]; } copy]]];
    }
    [self sheet:@"Lọc sách" items:items from:_filterBtn];
}

- (void)setFilterIndex:(NSInteger)i {
    _filterIndex = i;
    NSArray *f = [GBFilters() objectAtIndex:i];
    _list.filter = [f objectAtIndex:1];
    NSString *fmt = [f objectAtIndex:2];
    _list.fmt = fmt.length ? fmt : nil;
    [self updateFilterTitle];
    [_list reload];
}

- (void)openTopics {
    GMTopicsViewController *t = [[GMTopicsViewController alloc] initWithStyle:UITableViewStylePlain];
    t.peer = _source;
    t.title = self.title;
    [self.navigationController pushViewController:t animated:YES];
}

- (void)searchBar:(UISearchBar *)bar textDidChange:(NSString *)text {
    [_searchTimer invalidate];
    _searchTimer = [NSTimer scheduledTimerWithTimeInterval:0.3 target:self selector:@selector(runSearch) userInfo:nil repeats:NO];
}

- (void)runSearch {
    if (_list) {
        _list.query = _search.text;
        [_list reload];
    } else {
        [self filterItems];
    }
}

- (void)searchBarSearchButtonClicked:(UISearchBar *)bar {
    [bar resignFirstResponder];
}

- (void)scrollViewWillBeginDragging:(UIScrollView *)sv {
    [_search resignFirstResponder];
}

- (void)sortChanged {
    NSArray *sorts = @[@"date", @"title", @"author"];
    _list.sort = [sorts objectAtIndex:MAX(0, _sortCtl.selectedSegmentIndex)];
    if (_kind != GBBooksOffline) [[NSUserDefaults standardUserDefaults] setObject:_list.sort forKey:@"books.sort"];
    [_list reload];
}

- (void)refresh {
    NSDictionary *a = _kind == GBBooksSource && _source ? @{@"peer": _source} : @{};
    [GMCore call:@"sync.start" args:a done:^(id ok, NSDictionary *err) {
        if (err) [GMTheme toast:GMErrorText(err) in:self.view];
        else if (![GMVal(ok, @"started") boolValue]) [GMTheme toast:@"Đang quét rồi." in:self.view];
        else [GMTheme toast:@"Đang tìm sách mới…" in:self.view];
        GM_IOS6_BEGIN
        if (_refresh) [(UIRefreshControl *)_refresh endRefreshing];
        GM_IOS6_END
    }];
}

#pragma mark - Sự kiện

- (void)scheduleReload {
    if (!_list) return;
    [_reloadTimer invalidate];
    _reloadTimer = [NSTimer scheduledTimerWithTimeInterval:1.0 target:_list selector:@selector(reload) userInfo:nil repeats:NO];
}

- (void)coreEvent:(NSNotification *)n {
    NSString *e = GMVal(n.userInfo, @"e");
    if ([e isEqual:@"library.changed"] || [e isEqual:@"cache.cleared"]) {
        [self scheduleReload];
        if (_kind == GBBooksCollection && [e isEqual:@"cache.cleared"]) [self loadItems];
    } else if ([e isEqual:@"sync.state"]) {
        _sync = n.userInfo;
        [self updateStatus];
        if (![GMVal(_sync, @"running") boolValue]) [self scheduleReload];
    } else if ([e isEqual:@"dl.progress"] || [e isEqual:@"file.progress"]) {
        NSString *key = GMVal(n.userInfo, @"key");
        double total = [GMVal(n.userInfo, @"total") doubleValue];
        if (key && total > 0) [_progress setObject:@([GMVal(n.userInfo, @"have") doubleValue] / total) forKey:key];
        [self reloadVisible];
    } else if ([e isEqual:@"dl.done"] || [e isEqual:@"file.done"] || [e isEqual:@"dl.error"] || [e isEqual:@"file.error"]) {
        NSString *key = GMVal(n.userInfo, @"key");
        if (key) [_progress removeObjectForKey:key];
        if (_kind == GBBooksOffline) [self scheduleReload];
        else if (_list) {
            NSDictionary *b = [[_list knownInfo] objectForKey:key ?: @""];
            if (b && ([e isEqual:@"dl.done"] || [e isEqual:@"file.done"])) {
                NSMutableDictionary *m = [b mutableCopy];
                NSString *dl = GMVal(b, @"dl");
                [m setObject:([dl isEqual:@"queued"] || [e isEqual:@"dl.done"]) ? @"offline" : @"cached" forKey:@"dl"];
                [_list replaceBook:m];
            }
            [self reloadVisible];
        } else {
            [self loadItems];
        }
        if ([e isEqual:@"dl.error"] && self.view.window) [GMTheme toast:GMErrorText(n.userInfo) in:self.view];
    }
}

@end
