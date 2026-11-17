#import "GBReaderParts.h"
#import "GMAppDelegate.h"
#import "GMArt.h"
#import "GMTheme.h"

#pragma mark - Cài đặt màn đọc

static NSUserDefaults *GBD(void) {
    return [NSUserDefaults standardUserDefaults];
}

static BOOL GBPad(void) {
    return UI_USER_INTERFACE_IDIOM() == UIUserInterfaceIdiomPad;
}

@implementation GBReaderPrefs

+ (NSArray *)fonts {
    // Đã dò từng chữ tiếng Việt trên iPad mini 1 (DECISIONS B8); hệ số cỡ như bảng FontPresets của iBooks (Literata chữ to hơn)
    return @[
        @[@"Georgia", @"Georgia", @1.0], @[@"Literata", @"Literata", @0.92], @[@"Times New Roman", @"Times New Roman", @1.08],
        @[@"Baskerville", @"Baskerville", @1.08], @[@"Cochin", @"Cochin", @1.1], @[@"Helvetica Neue", @"Helvetica Neue", @0.96],
        @[@"Avenir Next", @"Avenir Next", @0.96], @[@"Verdana", @"Verdana", @0.9], @[@"Theo sách", @"", @1.0],
    ];
}

+ (NSArray *)sizes {
    return @[@13, @14, @15, @16, @17, @18, @20, @22, @24, @27, @30, @34];
}

+ (NSInteger)sizeIndex {
    id v = [GBD() objectForKey:@"gb.fs"];
    NSInteger i = v ? [v integerValue] : (GBPad() ? 6 : 5);
    return MAX(0, MIN((NSInteger)[self sizes].count - 1, i));
}

+ (void)setSizeIndex:(NSInteger)i {
    [GBD() setInteger:MAX(0, MIN((NSInteger)[self sizes].count - 1, i)) forKey:@"gb.fs"];
}

+ (NSInteger)fontIndex {
    NSInteger i = [GBD() integerForKey:@"gb.ff"];
    return MAX(0, MIN((NSInteger)[self fonts].count - 1, i));
}

+ (void)setFontIndex:(NSInteger)i {
    [GBD() setInteger:i forKey:@"gb.ff"];
}

/// Nền trang nhớ riêng cho giao diện sáng (mặc định Trắng) và giao diện tối của app (mặc định Đêm): bật tối là trang tối theo.
+ (NSString *)theme {
    BOOL dark = [GMTheme dark];
    NSString *t = [GBD() stringForKey:dark ? @"gb.theme.dark" : @"gb.theme"];
    return t.length ? t : (dark ? @"night" : @"day");
}

+ (void)setTheme:(NSString *)t {
    [GBD() setObject:t forKey:[GMTheme dark] ? @"gb.theme.dark" : @"gb.theme"];
}

+ (NSInteger)lineIndex {
    id v = [GBD() objectForKey:@"gb.lh"];
    return v ? MAX(0, MIN(2, [v integerValue])) : 1;
}

+ (void)setLineIndex:(NSInteger)i {
    [GBD() setInteger:i forKey:@"gb.lh"];
}

+ (NSInteger)marginIndex {
    id v = [GBD() objectForKey:@"gb.margin"];
    return v ? MAX(0, MIN(2, [v integerValue])) : 1;
}

+ (void)setMarginIndex:(NSInteger)i {
    [GBD() setInteger:i forKey:@"gb.margin"];
}

+ (BOOL)justify {
    id v = [GBD() objectForKey:@"gb.just"];
    return v ? [v boolValue] : YES;
}

+ (void)setJustify:(BOOL)on {
    [GBD() setBool:on forKey:@"gb.just"];
}

+ (NSInteger)weight {
    return MAX(0, MIN(2, [GBD() integerForKey:@"gb.weight"]));
}

+ (void)setWeight:(NSInteger)w {
    [GBD() setInteger:w forKey:@"gb.weight"];
}

+ (NSString *)turn {
    return [GBD() stringForKey:@"gb.turn"] ?: @"curl";
}

+ (void)setTurn:(NSString *)t {
    [GBD() setObject:t forKey:@"gb.turn"];
}

+ (BOOL)publisher {
    return [GBD() boolForKey:@"gb.pub"];
}

+ (void)setPublisher:(BOOL)on {
    [GBD() setBool:on forKey:@"gb.pub"];
}

+ (BOOL)twoPages {
    id v = [GBD() objectForKey:@"gb.two"];
    return v ? [v boolValue] : YES;
}

+ (void)setTwoPages:(BOOL)on {
    [GBD() setBool:on forKey:@"gb.two"];
}

+ (CGFloat)dim {
    return MAX(0, MIN(0.6, [GBD() floatForKey:@"gb.dim"]));
}

+ (void)setDim:(CGFloat)d {
    [GBD() setFloat:d forKey:@"gb.dim"];
}

+ (CGFloat)warm {
    return MAX(0, MIN(1, [GBD() floatForKey:@"gb.warm"]));
}

+ (void)setWarm:(CGFloat)w {
    [GBD() setFloat:MAX(0, MIN(1, w)) forKey:@"gb.warm"];
}

+ (CGFloat)light {
    CGFloat d = [self dim];
    return d > 0 ? -d : [UIScreen mainScreen].brightness;
}

+ (void)setLight:(CGFloat)v {
    v = MAX(-0.6, MIN(1, v));
    if (v >= 0) {
        [UIScreen mainScreen].brightness = v;
        [self setDim:0];
    } else {
        [UIScreen mainScreen].brightness = 0;
        [self setDim:-v];
    }
}

+ (UIColor *)paperFor:(NSString *)t {
    if ([t isEqual:@"sepia"]) return [UIColor colorWithRed:0xF4 / 255.0 green:0xEC / 255.0 blue:0xD8 / 255.0 alpha:1];
    if ([t isEqual:@"gray"]) return [UIColor colorWithRed:0x5A / 255.0 green:0x5A / 255.0 blue:0x5C / 255.0 alpha:1];
    if ([t isEqual:@"night"]) return [UIColor colorWithRed:0x12 / 255.0 green:0x12 / 255.0 blue:0x12 / 255.0 alpha:1];
    return [UIColor whiteColor];
}

+ (UIColor *)inkFor:(NSString *)t {
    if ([t isEqual:@"sepia"]) return [UIColor colorWithRed:0x5B / 255.0 green:0x46 / 255.0 blue:0x36 / 255.0 alpha:1];
    if ([t isEqual:@"gray"]) return [UIColor colorWithWhite:0.91 alpha:1];
    if ([t isEqual:@"night"]) return [UIColor colorWithWhite:0.78 alpha:1];
    return [UIColor colorWithWhite:0.11 alpha:1];
}

@end

#pragma mark - Trang ảnh

@implementation GBPageVC {
    UIImageView *_iv;
}

+ (GBPageVC *)pageWithImage:(UIImage *)img page:(NSInteger)page paper:(UIColor *)paper {
    GBPageVC *p = [[GBPageVC alloc] init];
    p.image = img;
    p.page = page;
    p.paper = paper;
    return p;
}

- (void)viewDidLoad {
    [super viewDidLoad];
    self.view.backgroundColor = _paper ?: [UIColor whiteColor];
    _iv = [[UIImageView alloc] initWithFrame:self.view.bounds];
    _iv.autoresizingMask = UIViewAutoresizingFlexibleWidth | UIViewAutoresizingFlexibleHeight;
    _iv.contentMode = UIViewContentModeTopLeft;
    _iv.image = _image;
    [self.view addSubview:_iv];
}

- (void)setImage:(UIImage *)image {
    _image = image;
    _iv.image = image;
}

@end

#pragma mark - Mục lục, dấu trang

@implementation GBTocViewController {
    UISegmentedControl *_seg;
}

- (void)viewDidLoad {
    [super viewDidLoad];
    _seg = [[UISegmentedControl alloc] initWithItems:@[@"Mục lục", @"Dấu trang"]];
    _seg.segmentedControlStyle = UISegmentedControlStyleBar;
    _seg.selectedSegmentIndex = 0;
    [_seg addTarget:self action:@selector(segChanged) forControlEvents:UIControlEventValueChanged];
    self.navigationItem.titleView = _seg;
    self.tableView.rowHeight = 44;
    if (!GBPad()) {
        self.navigationItem.leftBarButtonItem =
            [[UIBarButtonItem alloc] initWithTitle:@"Đóng" style:UIBarButtonItemStyleBordered target:self action:@selector(close)];
    }
}

- (void)viewDidAppear:(BOOL)animated {
    [super viewDidAppear:animated];
    // cuộn tới chương đang đọc
    if (_seg.selectedSegmentIndex == 0) {
        NSInteger row = -1;
        for (NSUInteger i = 0; i < _toc.count; i++) {
            id s = GMVal([_toc objectAtIndex:i], @"s");
            if (s && [s integerValue] <= _currentSpine) row = i;
        }
        if (row >= 0) {
            [self.tableView scrollToRowAtIndexPath:[NSIndexPath indexPathForRow:row inSection:0] atScrollPosition:UITableViewScrollPositionMiddle animated:NO];
        }
    }
}

- (void)close {
    [self dismissViewControllerAnimated:YES completion:nil];
}

- (void)segChanged {
    [self.tableView reloadData];
}

- (NSInteger)tableView:(UITableView *)tv numberOfRowsInSection:(NSInteger)s {
    return _seg.selectedSegmentIndex == 0 ? _toc.count : MAX(1, _marks.count);
}

- (UITableViewCell *)tableView:(UITableView *)tv cellForRowAtIndexPath:(NSIndexPath *)ip {
    UITableViewCell *c = [tv dequeueReusableCellWithIdentifier:@"t"] ?:
        [[UITableViewCell alloc] initWithStyle:UITableViewCellStyleSubtitle reuseIdentifier:@"t"];
    c.detailTextLabel.text = nil;
    c.textLabel.textColor = [UIColor darkTextColor];
    c.selectionStyle = UITableViewCellSelectionStyleBlue;
    if (_seg.selectedSegmentIndex == 0) {
        NSDictionary *t = [_toc objectAtIndex:ip.row];
        NSInteger depth = [GMVal(t, @"depth") integerValue];
        c.indentationLevel = MIN(depth, 4);
        c.indentationWidth = 18;
        c.textLabel.text = GMVal(t, @"title");
        id s = GMVal(t, @"s");
        BOOL here = NO;
        if (s) {
            // chương đang đọc: mục cuối cùng có s ≤ chương hiện tại
            NSInteger next = ip.row + 1 < (NSInteger)_toc.count ? [GMVal([_toc objectAtIndex:ip.row + 1], @"s") integerValue] : NSIntegerMax;
            here = [s integerValue] <= _currentSpine && next > _currentSpine;
        }
        c.textLabel.font = here ? [UIFont boldSystemFontOfSize:16] : [UIFont systemFontOfSize:16];
        c.textLabel.textColor = here ? [GMTheme accent] : [UIColor darkTextColor];
        return c;
    }
    c.indentationLevel = 0;
    if (!_marks.count) {
        c.textLabel.text = @"Chưa có dấu trang";
        c.detailTextLabel.text = @"Chạm biểu tượng ruy-băng trên thanh trên để đặt dấu trang.";
        c.textLabel.textColor = [UIColor grayColor];
        c.selectionStyle = UITableViewCellSelectionStyleNone;
        return c;
    }
    NSDictionary *m = [_marks objectAtIndex:ip.row];
    NSString *snip = GMVal(GMVal(m, @"pos"), @"t");
    c.textLabel.text = snip.length ? [NSString stringWithFormat:@"«%@…»", snip] : @"Dấu trang";
    c.textLabel.font = [UIFont systemFontOfSize:15];
    NSDate *d = [NSDate dateWithTimeIntervalSince1970:[GMVal(m, @"date") doubleValue]];
    NSDateFormatter *f = [[NSDateFormatter alloc] init];
    f.dateFormat = @"dd/MM/yyyy HH:mm";
    NSString *ch = GMVal(m, @"chapter");
    c.detailTextLabel.text = ch.length ? [NSString stringWithFormat:@"%@ · %@", ch, [f stringFromDate:d]] : [f stringFromDate:d];
    return c;
}

- (void)tableView:(UITableView *)tv didSelectRowAtIndexPath:(NSIndexPath *)ip {
    [tv deselectRowAtIndexPath:ip animated:YES];
    if (_seg.selectedSegmentIndex == 0) {
        NSDictionary *t = [_toc objectAtIndex:ip.row];
        if (!GMVal(t, @"s")) return;
        if (_pick) _pick(@{@"s": GMVal(t, @"s"), @"frag": GMVal(t, @"frag") ?: @""});
    } else if (_marks.count) {
        if (_pick) _pick(@{@"pos": GMVal([_marks objectAtIndex:ip.row], @"pos") ?: @{}});
    }
}

- (BOOL)tableView:(UITableView *)tv canEditRowAtIndexPath:(NSIndexPath *)ip {
    return _seg.selectedSegmentIndex == 1 && _marks.count;
}

- (void)tableView:(UITableView *)tv commitEditingStyle:(UITableViewCellEditingStyle)style forRowAtIndexPath:(NSIndexPath *)ip {
    if (style != UITableViewCellEditingStyleDelete || ip.row >= (NSInteger)_marks.count) return;
    NSDictionary *m = [_marks objectAtIndex:ip.row];
    NSMutableArray *v = [_marks mutableCopy];
    [v removeObjectAtIndex:ip.row];
    _marks = v;
    if (_removeMark) _removeMark(m);
    [self.tableView reloadData];
}

@end

#pragma mark - Bảng «Aa»

@implementation GBStyleViewController {
    UISlider *_bright, *_warm;
}

- (instancetype)initWithStyle:(UITableViewStyle)style {
    return [super initWithStyle:UITableViewStyleGrouped];
}

- (void)viewDidLoad {
    [super viewDidLoad];
    self.title = @"Kiểu chữ";
    if (!GBPad()) {
        self.navigationItem.rightBarButtonItem =
            [[UIBarButtonItem alloc] initWithTitle:@"Xong" style:UIBarButtonItemStyleDone target:self action:@selector(close)];
    }
}

- (void)close {
    [self dismissViewControllerAnimated:YES completion:nil];
}

- (void)fire {
    [[NSUserDefaults standardUserDefaults] synchronize];
    if (_changed) _changed();
    [self.tableView reloadData];
}

/// 0 độ sáng · 1 cỡ chữ · 2 nền · 3 font · 4 dàn trang · 5 lật trang
- (NSInteger)numberOfSectionsInTableView:(UITableView *)tv {
    return 6;
}

- (NSString *)tableView:(UITableView *)tv titleForHeaderInSection:(NSInteger)s {
    return [@[@"Độ sáng · ánh sáng ấm", @"Cỡ chữ", @"Nền", @"Font", @"Dàn trang", @"Lật trang"] objectAtIndex:s];
}

- (NSInteger)tableView:(UITableView *)tv numberOfRowsInSection:(NSInteger)s {
    switch (s) {
        case 0: return 2;
        case 2: return 2;
        case 3: return [GBReaderPrefs fonts].count;
        case 4: return GBPad() ? 5 : 4;
        default: return 1;
    }
}

- (UISegmentedControl *)seg:(NSArray *)items sel:(NSInteger)sel action:(SEL)a width:(CGFloat)w {
    UISegmentedControl *s = [[UISegmentedControl alloc] initWithItems:items];
    s.segmentedControlStyle = UISegmentedControlStyleBar;
    s.selectedSegmentIndex = sel;
    s.frame = CGRectMake(0, 0, w, 30);
    [s addTarget:self action:a forControlEvents:UIControlEventValueChanged];
    return s;
}

- (UITableViewCell *)tableView:(UITableView *)tv cellForRowAtIndexPath:(NSIndexPath *)ip {
    UITableViewCell *c = [[UITableViewCell alloc] initWithStyle:UITableViewCellStyleValue1 reuseIdentifier:nil];
    c.selectionStyle = UITableViewCellSelectionStyleNone;
    CGFloat w = tv.bounds.size.width - (GBPad() ? 90 : 40);
    switch (ip.section) {
        case 0: {
            if (ip.row == 1) {
                // Ánh sáng ấm: chữ ngắn bên trái, thanh kéo bên phải (0 = tắt)
                _warm = [[UISlider alloc] initWithFrame:CGRectMake(0, 0, w - 70, 30)];
                _warm.minimumValue = 0;
                _warm.maximumValue = 1;
                _warm.value = [GBReaderPrefs warm];
                _warm.minimumTrackTintColor = [UIColor colorWithRed:1 green:0.6 blue:0.2 alpha:1];
                [_warm addTarget:self action:@selector(warmChanged) forControlEvents:UIControlEventValueChanged];
                c.accessoryView = _warm;
                c.textLabel.text = @"Ấm";
                break;
            }
            _bright = [[UISlider alloc] initWithFrame:CGRectMake(0, 0, w, 30)];
            _bright.minimumValue = -0.6;  // dưới 0: lớp tối thêm (tối hơn mức thấp nhất của máy)
            _bright.maximumValue = 1;
            _bright.value = [GBReaderPrefs light];
            [_bright addTarget:self action:@selector(brightChanged) forControlEvents:UIControlEventValueChanged];
            c.accessoryView = _bright;
            c.textLabel.text = @"☀︎";
            break;
        }
        case 1: {
            UISegmentedControl *s = [self seg:@[@"A−", @"A+"] sel:-1 action:@selector(sizeTapped:) width:140];
            s.momentary = YES;
            c.accessoryView = s;
            c.textLabel.text = [NSString stringWithFormat:@"Nấc %ld / %lu", (long)[GBReaderPrefs sizeIndex] + 1, (unsigned long)[GBReaderPrefs sizes].count];
            break;
        }
        case 2: {
            if (ip.row == 1) {
                // Cả app tối (thư viện, bảng, thanh) + trang theo nền đã chọn cho giao diện tối (mặc định Đêm)
                c.textLabel.text = [GMTheme mode] == GMThemeAuto ? @"Giao diện tối (đang tự động)" : @"Giao diện tối";
                UISwitch *sw = [[UISwitch alloc] init];
                sw.on = [GMTheme dark];
                [sw addTarget:self action:@selector(darkChanged:) forControlEvents:UIControlEventValueChanged];
                c.accessoryView = sw;
                break;
            }
            NSArray *ts = @[@"day", @"sepia", @"gray", @"night"];
            UISegmentedControl *s = [self seg:@[@"Trắng", @"Giấy ngà", @"Xám", @"Đêm"]
                                          sel:[ts indexOfObject:[GBReaderPrefs theme]] action:@selector(themeChanged:) width:w];
            c.accessoryView = s;
            break;
        }
        case 3: {
            NSArray *f = [[GBReaderPrefs fonts] objectAtIndex:ip.row];
            c.textLabel.text = [f objectAtIndex:0];
            NSString *fam = [f objectAtIndex:1];
            UIFont *font = fam.length ? [UIFont fontWithName:fam size:17] : nil;
            if (!font && fam.length) {
                // tên họ font → kiểu thường
                NSArray *names = [UIFont fontNamesForFamilyName:fam];
                if (names.count) font = [UIFont fontWithName:[names objectAtIndex:0] size:17];
            }
            c.textLabel.font = font ?: [UIFont systemFontOfSize:17];
            c.accessoryType = ip.row == [GBReaderPrefs fontIndex] ? UITableViewCellAccessoryCheckmark : UITableViewCellAccessoryNone;
            c.selectionStyle = UITableViewCellSelectionStyleBlue;
            break;
        }
        case 4: {
            if (ip.row == 0) {
                c.textLabel.text = @"Giãn dòng";
                c.accessoryView = [self seg:@[@"Hẹp", @"Vừa", @"Rộng"] sel:[GBReaderPrefs lineIndex] action:@selector(lineChanged:) width:170];
            } else if (ip.row == 1) {
                c.textLabel.text = @"Lề";
                c.accessoryView = [self seg:@[@"Hẹp", @"Vừa", @"Rộng"] sel:[GBReaderPrefs marginIndex] action:@selector(marginChanged:) width:170];
            } else if (ip.row == 2) {
                c.textLabel.text = @"Chữ đậm hơn";
                c.accessoryView = [self seg:@[@"Thường", @"Hơi đậm", @"Đậm"] sel:[GBReaderPrefs weight] action:@selector(weightChanged:) width:190];
            } else if (ip.row == 3) {
                c.textLabel.text = @"Căn đều hai bên";
                UISwitch *sw = [[UISwitch alloc] init];
                sw.on = [GBReaderPrefs justify];
                [sw addTarget:self action:@selector(justChanged:) forControlEvents:UIControlEventValueChanged];
                c.accessoryView = sw;
            } else {
                c.textLabel.text = @"Ngang: 2 trang";
                UISwitch *sw = [[UISwitch alloc] init];
                sw.on = [GBReaderPrefs twoPages];
                [sw addTarget:self action:@selector(twoChanged:) forControlEvents:UIControlEventValueChanged];
                c.accessoryView = sw;
            }
            break;
        }
        case 5: {
            NSArray *ts = @[@"curl", @"slide", @"none"];
            c.accessoryView = [self seg:@[@"Cong (iBooks)", @"Trượt", @"Không"] sel:[ts indexOfObject:[GBReaderPrefs turn]]
                                 action:@selector(turnChanged:) width:w];
            break;
        }
    }
    return c;
}

- (void)tableView:(UITableView *)tv didSelectRowAtIndexPath:(NSIndexPath *)ip {
    [tv deselectRowAtIndexPath:ip animated:YES];
    if (ip.section == 3) {
        [GBReaderPrefs setFontIndex:ip.row];
        [self fire];
    }
}

- (void)brightChanged {
    [GBReaderPrefs setLight:_bright.value];
    if (_lightChanged) _lightChanged();
    else if (_changed) _changed();
}

- (void)warmChanged {
    [GBReaderPrefs setWarm:_warm.value];
    if (_lightChanged) _lightChanged();
    else if (_changed) _changed();
}

- (void)sizeTapped:(UISegmentedControl *)s {
    NSInteger i = [GBReaderPrefs sizeIndex] + (s.selectedSegmentIndex == 0 ? -1 : 1);
    [GBReaderPrefs setSizeIndex:i];
    [self fire];
}

/// Bật / tắt giao diện tối của cả app ngay trong bảng Aa (chế độ Tự động → thành Sáng / Tối cố định; đổi lại ở Cài đặt).
- (void)darkChanged:(UISwitch *)sw {
    [GMTheme setMode:sw.on ? GMThemeDark : GMThemeLight];
    [(GMAppDelegate *)[UIApplication sharedApplication].delegate reloadTheme];
    if (self.tableView.window) [GMTheme restyleTable:self.tableView];
}

- (void)themeChanged:(UISegmentedControl *)s {
    [GBReaderPrefs setTheme:[@[@"day", @"sepia", @"gray", @"night"] objectAtIndex:MAX(0, s.selectedSegmentIndex)]];
    [self fire];
}

- (void)lineChanged:(UISegmentedControl *)s {
    [GBReaderPrefs setLineIndex:s.selectedSegmentIndex];
    [self fire];
}

- (void)marginChanged:(UISegmentedControl *)s {
    [GBReaderPrefs setMarginIndex:s.selectedSegmentIndex];
    [self fire];
}

- (void)weightChanged:(UISegmentedControl *)s {
    [GBReaderPrefs setWeight:s.selectedSegmentIndex];
    [self fire];
}

- (void)justChanged:(UISwitch *)s {
    [GBReaderPrefs setJustify:s.on];
    [self fire];
}

- (void)twoChanged:(UISwitch *)s {
    [GBReaderPrefs setTwoPages:s.on];
    [self fire];
}

- (void)turnChanged:(UISegmentedControl *)s {
    [GBReaderPrefs setTurn:[@[@"curl", @"slide", @"none"] objectAtIndex:MAX(0, s.selectedSegmentIndex)]];
    if (_changed) _changed();
}

@end
