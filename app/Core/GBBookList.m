#import "GBBookList.h"
#import "GMCore.h"

static const NSInteger kPage = 100;

@implementation GBBookList {
    NSMutableDictionary *_pages;  // số trang → [sách]
    NSMutableSet *_loading;
    NSInteger _gen;
}

- (instancetype)init {
    if ((self = [super init])) {
        _pages = [NSMutableDictionary dictionary];
        _loading = [NSMutableSet set];
        _sort = @"date";
        _filter = @"all";
        _sections = @[];
    }
    return self;
}

- (NSDictionary *)args:(NSInteger)offset {
    NSMutableDictionary *a = [NSMutableDictionary dictionaryWithDictionary:@{
        @"q": _query ?: @"", @"sort": _sort ?: @"date", @"filter": _filter ?: @"all", @"offset": @(offset), @"limit": @(kPage),
    }];
    if (_source.length) [a setObject:_source forKey:@"source"];
    if (_topic) [a setObject:_topic forKey:@"topic"];
    if (_fmt.length) [a setObject:_fmt forKey:@"fmt"];
    return a;
}

- (void)reload {
    _gen++;
    [_loading removeAllObjects];
    [self fetchPage:0 reset:YES];
}

- (void)fetchPage:(NSInteger)page reset:(BOOL)reset {
    NSNumber *p = @(page);
    if ([_loading containsObject:p]) return;
    [_loading addObject:p];
    NSInteger gen = _gen;
    [GMCore call:@"library.query" args:[self args:page * kPage] done:^(id ok, NSDictionary *err) {
        if (gen != _gen) return;
        [_loading removeObject:p];
        if (err || ![ok isKindOfClass:[NSDictionary class]]) return;
        if (reset) [_pages removeAllObjects];
        _total = [[ok objectForKey:@"total"] integerValue];
        NSArray *secs = [ok objectForKey:@"sections"];
        if (reset || page == 0) _sections = [secs isKindOfClass:[NSArray class]] ? secs : @[];
        NSArray *items = [ok objectForKey:@"items"];
        if ([items isKindOfClass:[NSArray class]]) [_pages setObject:items forKey:p];
        _loaded = YES;
        if (_changed) _changed();
    }];
}

- (NSDictionary *)bookAt:(NSInteger)i {
    if (i < 0 || i >= _total) return nil;
    NSInteger page = i / kPage;
    NSArray *items = [_pages objectForKey:@(page)];
    if (!items) {
        [self fetchPage:page reset:NO];
        return nil;
    }
    NSInteger j = i % kPage;
    if (j > kPage - 20 && (page + 1) * kPage < _total && ![_pages objectForKey:@(page + 1)]) [self fetchPage:page + 1 reset:NO];
    return j < (NSInteger)items.count ? [items objectAtIndex:j] : nil;
}

- (void)replaceBook:(NSDictionary *)book {
    NSString *k = [book objectForKey:@"key"];
    for (NSNumber *p in [_pages allKeys]) {
        NSMutableArray *items = [[_pages objectForKey:p] mutableCopy];
        for (NSUInteger i = 0; i < items.count; i++) {
            if ([[[items objectAtIndex:i] objectForKey:@"key"] isEqual:k]) {
                [items replaceObjectAtIndex:i withObject:book];
                [_pages setObject:items forKey:p];
                return;
            }
        }
    }
}

- (NSDictionary *)knownInfo {
    NSMutableDictionary *d = [NSMutableDictionary dictionary];
    for (NSArray *items in [_pages allValues]) {
        for (NSDictionary *s in items) {
            NSString *k = [s objectForKey:@"key"];
            if (k) [d setObject:s forKey:k];
        }
    }
    return d;
}

@end
