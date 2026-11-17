#import "GBLocalViewController.h"
#import "GBBookCell.h"
#import "GMRootController.h"
#import "GMTheme.h"

static NSSet *GBBookExts(void) {
    return [NSSet setWithObjects:@"epub", @"mobi", @"azw3", @"azw", @"prc", @"pdf", @"fb2", @"txt", nil];
}

@implementation GBLocalViewController {
    NSString *_path;
    NSArray *_dirs, *_files;  // tên
}

- (instancetype)initWithPath:(NSString *)path {
    if ((self = [super initWithStyle:UITableViewStylePlain])) {
        NSString *docs = [NSHomeDirectory() stringByAppendingPathComponent:@"Documents"];
        _path = path ?: docs;
        self.title = path ? [path lastPathComponent] : @"Sách trên máy";
    }
    return self;
}

- (void)viewDidLoad {
    [super viewDidLoad];
    self.tableView.rowHeight = [GBBookCell height];
    [self load];
}

- (void)viewWillAppear:(BOOL)animated {
    [super viewWillAppear:animated];
    [self.tableView reloadData];  // tiến độ đọc có thể vừa đổi
}

- (void)load {
    NSFileManager *fm = [NSFileManager defaultManager];
    NSArray *names = [[fm contentsOfDirectoryAtPath:_path error:NULL] sortedArrayUsingSelector:@selector(localizedCaseInsensitiveCompare:)];
    NSMutableArray *dirs = [NSMutableArray array], *files = [NSMutableArray array];
    NSSet *exts = GBBookExts();
    for (NSString *n in names) {
        if ([n hasPrefix:@"."]) continue;
        BOOL dir = NO;
        [fm fileExistsAtPath:[_path stringByAppendingPathComponent:n] isDirectory:&dir];
        if (dir) [dirs addObject:n];
        else if ([exts containsObject:[[n pathExtension] lowercaseString]]) [files addObject:n];
    }
    _dirs = dirs;
    _files = files;
    [self.tableView reloadData];
    if (!dirs.count && !files.count) {
        UILabel *l = [[UILabel alloc] initWithFrame:CGRectMake(20, 20, self.tableView.bounds.size.width - 40, 110)];
        l.text = [NSString stringWithFormat:@"Thư mục không có sách.\nChép tệp EPUB / MOBI / AZW3 vào\n%@", _path];
        l.numberOfLines = 0;
        l.textAlignment = GMAlignCenter;
        l.font = [UIFont systemFontOfSize:14];
        l.textColor = [GMTheme secondaryText];
        l.backgroundColor = [UIColor clearColor];
        l.autoresizingMask = UIViewAutoresizingFlexibleWidth;
        UIView *f = [[UIView alloc] initWithFrame:CGRectMake(0, 0, self.tableView.bounds.size.width, 140)];
        [f addSubview:l];
        self.tableView.tableFooterView = f;
    }
}

- (NSInteger)numberOfSectionsInTableView:(UITableView *)tv {
    return 2;
}

- (NSInteger)tableView:(UITableView *)tv numberOfRowsInSection:(NSInteger)s {
    return s == 0 ? _dirs.count : _files.count;
}

- (CGFloat)tableView:(UITableView *)tv heightForRowAtIndexPath:(NSIndexPath *)ip {
    return ip.section == 0 ? 48 : [GBBookCell height];
}

/// Thông tin sách từ tên tệp («Tên sách - Tác giả.epub»).
- (NSDictionary *)bookFor:(NSString *)name {
    NSString *path = [_path stringByAppendingPathComponent:name];
    NSString *ext = [[name pathExtension] lowercaseString];
    NSString *stem = [[name stringByDeletingPathExtension] stringByReplacingOccurrencesOfString:@"_" withString:@" "];
    NSString *title = stem, *author = @"";
    NSRange r = [stem rangeOfString:@" - " options:NSBackwardsSearch];
    if (r.location != NSNotFound && r.location > 0) {
        title = [stem substringToIndex:r.location];
        author = [stem substringFromIndex:r.location + 3];
    }
    NSDictionary *attrs = [[NSFileManager defaultManager] attributesOfItemAtPath:path error:NULL];
    return @{@"key": [@"file:" stringByAppendingString:path], @"title": title, @"author": author, @"ext": ext,
             @"size": @([attrs fileSize]), @"ok": @(GBReadableExt(ext)), @"local": @YES};
}

- (UITableViewCell *)tableView:(UITableView *)tv cellForRowAtIndexPath:(NSIndexPath *)ip {
    if (ip.section == 0) {
        UITableViewCell *c = [tv dequeueReusableCellWithIdentifier:@"dir"] ?:
            [[UITableViewCell alloc] initWithStyle:UITableViewCellStyleDefault reuseIdentifier:@"dir"];
        c.textLabel.text = [_dirs objectAtIndex:ip.row];
        c.textLabel.font = [UIFont boldSystemFontOfSize:15];
        c.imageView.image = [GMTheme folderIcon];
        c.accessoryType = UITableViewCellAccessoryDisclosureIndicator;
        return c;
    }
    GBBookCell *c = (GBBookCell *)[tv dequeueReusableCellWithIdentifier:@"book"];
    if (!c) c = [[GBBookCell alloc] initWithStyle:UITableViewCellStyleDefault reuseIdentifier:@"book"];
    [c setBook:[self bookFor:[_files objectAtIndex:ip.row]] progress:-1 showSource:NO];
    c.more = nil;
    return c;
}

- (void)tableView:(UITableView *)tv didSelectRowAtIndexPath:(NSIndexPath *)ip {
    [tv deselectRowAtIndexPath:ip animated:YES];
    if (ip.section == 0) {
        GBLocalViewController *v = [[GBLocalViewController alloc] initWithPath:[_path stringByAppendingPathComponent:[_dirs objectAtIndex:ip.row]]];
        [self.navigationController pushViewController:v animated:YES];
        return;
    }
    NSDictionary *b = [self bookFor:[_files objectAtIndex:ip.row]];
    if (![GMVal(b, @"ok") boolValue]) {
        [GMTheme toast:[NSString stringWithFormat:@"Chưa đọc được định dạng %@ — bản sau sẽ có.", [GMVal(b, @"ext") uppercaseString]] in:self.view];
        return;
    }
    [GMRoot() openBook:b];
}

@end
