#import "GBBookCell.h"
#import "GBCover.h"
#import "GMArt.h"
#import "GMImages.h"
#import "GMTheme.h"
#import <QuartzCore/QuartzCore.h>

static const CGFloat kCoverW = 44, kCoverH = 66;

BOOL GBReadableExt(NSString *ext) {
    static NSSet *ok;
    if (!ok) ok = [NSSet setWithObjects:@"epub", @"mobi", @"azw3", @"azw", @"prc", nil];
    return ext && [ok containsObject:[ext lowercaseString]];
}

@implementation GBBookCell {
    UIImageView *_cover, *_badge, *_dl;
    UILabel *_title, *_detail, *_caption;
    UIView *_bar, *_barFill;
    UIButton *_moreBtn;
    NSString *_key;
    double _readProgress;
}

+ (CGFloat)height {
    return 82;
}

- (instancetype)initWithStyle:(UITableViewCellStyle)style reuseIdentifier:(NSString *)rid {
    if ((self = [super initWithStyle:UITableViewCellStyleDefault reuseIdentifier:rid])) {
        _cover = [[UIImageView alloc] initWithFrame:CGRectMake(10, 8, kCoverW, kCoverH)];
        _cover.layer.shadowColor = [UIColor blackColor].CGColor;
        _cover.layer.shadowOpacity = 0.25;
        _cover.layer.shadowOffset = CGSizeMake(0, 1);
        _cover.layer.shadowRadius = 1.5;
        [self.contentView addSubview:_cover];
        _title = [[UILabel alloc] init];
        _title.font = [UIFont boldSystemFontOfSize:15];
        _title.backgroundColor = [UIColor clearColor];
        [self.contentView addSubview:_title];
        _detail = [[UILabel alloc] init];
        _detail.font = [UIFont systemFontOfSize:12.5];
        _detail.backgroundColor = [UIColor clearColor];
        [self.contentView addSubview:_detail];
        _caption = [[UILabel alloc] init];
        _caption.font = [UIFont systemFontOfSize:11.5];
        _caption.backgroundColor = [UIColor clearColor];
        [self.contentView addSubview:_caption];
        _badge = [[UIImageView alloc] init];
        [self.contentView addSubview:_badge];
        _dl = [[UIImageView alloc] init];
        [self.contentView addSubview:_dl];
        _bar = [[UIView alloc] init];
        _bar.backgroundColor = [[GMTheme accent] colorWithAlphaComponent:0.18];
        _bar.layer.cornerRadius = 1.5;
        [self.contentView addSubview:_bar];
        _barFill = [[UIView alloc] init];
        _barFill.backgroundColor = [GMTheme accent];
        _barFill.layer.cornerRadius = 1.5;
        [_bar addSubview:_barFill];
        _moreBtn = [UIButton buttonWithType:UIButtonTypeCustom];
        [_moreBtn addTarget:self action:@selector(moreTapped) forControlEvents:UIControlEventTouchUpInside];
        [self.contentView addSubview:_moreBtn];
    }
    return self;
}

- (void)moreTapped {
    if (_more) _more();
}

- (void)layoutSubviews {
    [super layoutSubviews];
    CGFloat w = self.contentView.bounds.size.width;
    _moreBtn.frame = CGRectMake(w - 40, 0, 40, [GBBookCell height]);
    CGFloat x = 10 + kCoverW + 12, right = w - 42;
    CGFloat bw = _badge.image ? _badge.image.size.width + 6 : 0;
    _title.frame = CGRectMake(x, 9, right - x - bw, 20);
    if (_badge.image) {
        CGSize s = _badge.image.size;
        _badge.frame = CGRectMake(right - s.width, 12, s.width, s.height);
    }
    CGFloat dx = _dl.image ? 16 : 0;
    _dl.frame = CGRectMake(x, 33, 12, 12);
    _detail.frame = CGRectMake(x + dx, 31, right - x - dx, 17);
    _caption.frame = CGRectMake(x, 49, right - x, 15);
    _bar.hidden = _readProgress <= 0;
    _bar.frame = CGRectMake(x, 68, MIN(140, right - x), 3);
    _barFill.frame = CGRectMake(0, 0, floor(_bar.bounds.size.width * MIN(1, _readProgress)), 3);
}

- (void)setLoading {
    _key = nil;
    _title.text = @"…";
    _detail.text = @"";
    _caption.text = @"";
    _cover.image = nil;
    _badge.image = nil;
    _dl.image = nil;
    _moreBtn.hidden = YES;
    _readProgress = 0;
    [self setNeedsLayout];
}

- (void)setBook:(NSDictionary *)b progress:(double)progress showSource:(BOOL)showSource {
    NSString *key = GMVal(b, @"key");
    BOOL missing = [GMVal(b, @"missing") boolValue];
    _moreBtn.hidden = missing;
    [_moreBtn setImage:[GMArt glyph:@"more" size:20 color:[GMTheme secondaryText]] forState:UIControlStateNormal];
    _title.text = GMVal(b, @"title") ?: @"";
    _title.textColor = missing ? [GMTheme secondaryText] : [GMTheme text];
    _detail.textColor = [GMTheme secondaryText];
    _caption.textColor = [[GMTheme secondaryText] colorWithAlphaComponent:0.8];
    NSMutableArray *parts = [NSMutableArray array];
    NSString *author = GMVal(b, @"author");
    if (author.length) [parts addObject:author];
    NSString *ext = [GMVal(b, @"ext") uppercaseString];
    if (ext.length) [parts addObject:ext];
    double size = [GMVal(b, @"size") doubleValue];
    if (size > 0) [parts addObject:[GMTheme sizeText:size]];
    NSString *src = GMVal(b, @"src");
    if (showSource && src.length) [parts addObject:src];
    if (missing) parts = [NSMutableArray arrayWithObject:@"Không còn trong thư viện"];
    if (progress >= 0 && progress < 1) [parts insertObject:[NSString stringWithFormat:@"Đang tải %.0f%%", progress * 100] atIndex:0];
    _detail.text = [parts componentsJoinedByString:@" · "];
    NSString *cap = GMVal(b, @"caption");
    _caption.text = [cap stringByReplacingOccurrencesOfString:@"\n" withString:@" "] ?: @"";
    if ([GMVal(b, @"finished") boolValue]) _caption.text = @"Đã đọc xong";
    BOOL ok = [GMVal(b, @"ok") boolValue];
    _badge.image = (!ok && !missing && ext.length) ? [GMArt badge:ext color:[UIColor colorWithRed:0.85 green:0.3 blue:0.25 alpha:1]] : nil;
    NSString *dl = GMVal(b, @"dl");
    UIColor *green = [GMTheme checkColor];
    if ([dl isEqual:@"offline"]) _dl.image = [GMArt glyph:@"downloaded" size:12 color:green];
    else if ([dl isEqual:@"queued"]) _dl.image = [GMArt glyph:@"download" size:12 color:[GMTheme secondaryText]];
    else if ([dl isEqual:@"cached"]) _dl.image = [GMArt glyph:@"downloaded" size:12 color:[GMTheme secondaryText]];
    else _dl.image = nil;
    _readProgress = [GMVal(b, @"progress") doubleValue];
    if (![key isEqual:_key]) {
        _key = key;
        CGSize cs = CGSizeMake(kCoverW, kCoverH);
        UIImage *cached = key ? [GMImages cachedCover:key size:kCoverH] : nil;
        _cover.image = cached ? [GBCover frame:cached size:cs] : [GBCover placeholderFor:b size:cs];
        if (key && !cached) {
            [GMImages cover:b size:kCoverH done:^(UIImage *img) {
                if ([_key isEqual:key]) _cover.image = [GBCover frame:img size:cs];
            }];
        }
    }
    [self setNeedsLayout];
}

@end
