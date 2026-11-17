#import "GBCollectionsViewController.h"
#import "GBBookActions.h"
#import "GBBooksViewController.h"
#import "GMArt.h"
#import "GMCore.h"
#import "GMTheme.h"

@implementation GBCollectionsViewController {
    NSDictionary *_data;
    NSArray *_lists;
}

- (void)viewDidLoad {
    [super viewDidLoad];
    self.title = @"Bộ sưu tập";
    self.tableView.rowHeight = 52;
    self.navigationItem.rightBarButtonItem =
        [[UIBarButtonItem alloc] initWithTitle:@"Tạo" style:UIBarButtonItemStyleBordered target:self action:@selector(create)];
}

- (void)viewWillAppear:(BOOL)animated {
    [super viewWillAppear:animated];
    [self load];
}

- (void)load {
    [GMCore call:@"col.list" args:nil done:^(id ok, NSDictionary *err) {
        if (![ok isKindOfClass:[NSDictionary class]]) return;
        _data = ok;
        _lists = GMVal(ok, @"lists") ?: @[];
        [self.tableView reloadData];
    }];
}

- (void)create {
    GMAskText(@"Bộ sưu tập mới", @"Tên bộ sưu tập", nil, ^(NSString *name) {
        [GMCore call:@"col.create" args:@{@"name": name} done:^(id ok, NSDictionary *err) {
            if (err) [GMTheme toast:GMErrorText(err) in:self.view];
            [self load];
        }];
    });
}

- (NSInteger)numberOfSectionsInTableView:(UITableView *)tv {
    return 2;
}

- (NSString *)tableView:(UITableView *)tv titleForHeaderInSection:(NSInteger)s {
    return s == 1 && _lists.count ? @"Bộ của bạn" : nil;
}

/// Tiêu đề nhóm: dải kính theo giao diện (dải xám mặc định của iOS 6 chói khi tối).
- (UIView *)tableView:(UITableView *)tv viewForHeaderInSection:(NSInteger)s {
    return [GMTheme plainHeader:[self tableView:tv titleForHeaderInSection:s]];
}

- (CGFloat)tableView:(UITableView *)tv heightForHeaderInSection:(NSInteger)s {
    return [self tableView:tv titleForHeaderInSection:s].length ? GMPlainHeaderHeight : 0;
}

- (NSInteger)tableView:(UITableView *)tv numberOfRowsInSection:(NSInteger)s {
    return s == 0 ? 4 : _lists.count;
}

- (UITableViewCell *)tableView:(UITableView *)tv cellForRowAtIndexPath:(NSIndexPath *)ip {
    UITableViewCell *c = [tv dequeueReusableCellWithIdentifier:@"c"] ?:
        [[UITableViewCell alloc] initWithStyle:UITableViewCellStyleSubtitle reuseIdentifier:@"c"];
    c.accessoryType = UITableViewCellAccessoryDisclosureIndicator;
    c.textLabel.font = [UIFont boldSystemFontOfSize:15];
    if (ip.section == 0) {
        NSArray *rows = @[@[@"Muốn đọc", @"want", @"plus"], @[@"Yêu thích", @"fav", @"heartFill"], @[@"Mở gần đây", @"recent", @"timer"],
                          @[@"Đã tải về máy", @"offline", @"downloaded"]];
        NSArray *r = [rows objectAtIndex:ip.row];
        c.textLabel.text = [r objectAtIndex:0];
        id n = GMVal(_data, [r objectAtIndex:1]);
        c.detailTextLabel.text = n ? [NSString stringWithFormat:@"%@ cuốn", n] : nil;
        c.imageView.image = [GMArt glyph:[r objectAtIndex:2] size:28 color:[GMTheme accent]];
        return c;
    }
    NSDictionary *p = [_lists objectAtIndex:ip.row];
    c.textLabel.text = GMVal(p, @"name");
    c.detailTextLabel.text = [NSString stringWithFormat:@"%@ cuốn", GMVal(p, @"count")];
    c.imageView.image = [GMArt glyph:@"queue" size:28 color:[GMTheme accent]];
    return c;
}

- (void)tableView:(UITableView *)tv didSelectRowAtIndexPath:(NSIndexPath *)ip {
    [tv deselectRowAtIndexPath:ip animated:YES];
    GBBooksViewController *v;
    if (ip.section == 0 && ip.row == 3) {
        v = [[GBBooksViewController alloc] initWithKind:GBBooksOffline];
    } else {
        v = [[GBBooksViewController alloc] initWithKind:GBBooksCollection];
        if (ip.section == 0) {
            v.collectionId = [@[@"want", @"fav", @"recent"] objectAtIndex:ip.row];
            v.title = [@[@"Muốn đọc", @"Yêu thích", @"Mở gần đây"] objectAtIndex:ip.row];
        } else {
            NSDictionary *p = [_lists objectAtIndex:ip.row];
            v.collectionId = GMVal(p, @"id");
            v.title = GMVal(p, @"name");
        }
    }
    [self.navigationController pushViewController:v animated:YES];
}

- (BOOL)tableView:(UITableView *)tv canEditRowAtIndexPath:(NSIndexPath *)ip {
    return ip.section == 1;
}

- (void)tableView:(UITableView *)tv commitEditingStyle:(UITableViewCellEditingStyle)style forRowAtIndexPath:(NSIndexPath *)ip {
    if (style != UITableViewCellEditingStyleDelete || ip.section != 1) return;
    NSDictionary *p = [_lists objectAtIndex:ip.row];
    [GMCore call:@"col.delete" args:@{@"id": GMVal(p, @"id")} done:^(id ok, NSDictionary *err) { [self load]; }];
}

@end
