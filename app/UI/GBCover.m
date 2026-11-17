#import "GBCover.h"
#import "GMTheme.h"

static UIColor *GBHex(int hex, CGFloat a) {
    return [UIColor colorWithRed:((hex >> 16) & 255) / 255.0 green:((hex >> 8) & 255) / 255.0 blue:(hex & 255) / 255.0 alpha:a];
}

@implementation GBCover

+ (UIImage *)placeholderFor:(NSDictionary *)book size:(CGSize)size {
    static NSCache *cache;
    if (!cache) cache = [[NSCache alloc] init];
    NSString *key = GMVal(book, @"key") ?: @"";
    NSString *title = GMVal(book, @"title") ?: @"";
    NSString *author = GMVal(book, @"author") ?: @"";
    NSString *ext = [GMVal(book, @"ext") uppercaseString] ?: @"";
    NSString *ck = [NSString stringWithFormat:@"%@|%.0fx%.0f|%@", key, size.width, size.height, title];
    UIImage *hit = [cache objectForKey:ck];
    if (hit) return hit;
    static int pairs[][2] = {
        {0x7B4B94, 0x4A2C5E}, {0xB5523B, 0x7A2E22}, {0x2F6E8E, 0x1B435A}, {0x3E7D4F, 0x22492E},
        {0xB98A3A, 0x7A5A1F}, {0x8E3B5F, 0x5A2240}, {0x4F5D75, 0x2D3646}, {0x2E7F7A, 0x1A4C49},
    };
    NSUInteger h = 0;
    for (NSUInteger i = 0; i < key.length; i++) h = h * 31 + [key characterAtIndex:i];
    int *pair = pairs[h % 8];
    UIGraphicsBeginImageContextWithOptions(size, NO, 0);
    CGContextRef ctx = UIGraphicsGetCurrentContext();
    CGFloat w = size.width, hh = size.height;
    CGRect r = CGRectMake(0, 0, w, hh);
    [[UIBezierPath bezierPathWithRoundedRect:r cornerRadius:MAX(1.5, w * 0.03)] addClip];
    CGColorSpaceRef cs = CGColorSpaceCreateDeviceRGB();
    NSArray *cols = @[(__bridge id)GBHex(pair[0], 1).CGColor, (__bridge id)GBHex(pair[1], 1).CGColor];
    CGGradientRef g = CGGradientCreateWithColors(cs, (__bridge CFArrayRef)cols, NULL);
    CGContextDrawLinearGradient(ctx, g, CGPointZero, CGPointMake(w * 0.4, hh), 0);
    CGGradientRelease(g);
    CGColorSpaceRelease(cs);
    // gáy sách
    [GBHex(0x000000, 0.22) setFill];
    CGContextFillRect(ctx, CGRectMake(0, 0, MAX(2, w * 0.07), hh));
    [GBHex(0xFFFFFF, 0.12) setFill];
    CGContextFillRect(ctx, CGRectMake(MAX(2, w * 0.07), 0, 1, hh));
    // khung chữ
    if (w >= 40) {
        CGFloat pad = w * 0.14;
        CGFloat tw = w - pad * 1.6;
        UIFont *tf = [UIFont fontWithName:@"Georgia-Bold" size:MAX(7, w * 0.13)] ?: [UIFont boldSystemFontOfSize:MAX(7, w * 0.13)];
        [[UIColor whiteColor] set];
        CGRect tr = CGRectMake(pad, hh * 0.14, tw, hh * 0.5);
        [title drawInRect:tr withFont:tf lineBreakMode:GMTailTruncation alignment:GMAlignCenter];
        if (author.length) {
            UIFont *af = [UIFont fontWithName:@"Georgia-Italic" size:MAX(6, w * 0.085)] ?: [UIFont italicSystemFontOfSize:MAX(6, w * 0.085)];
            [GBHex(0xFFFFFF, 0.85) set];
            [author drawInRect:CGRectMake(pad, hh * 0.68, tw, hh * 0.16) withFont:af lineBreakMode:GMTailTruncation alignment:GMAlignCenter];
        }
        if (ext.length && w >= 60) {
            UIFont *ef = [UIFont boldSystemFontOfSize:MAX(6, w * 0.06)];
            [GBHex(0xFFFFFF, 0.6) set];
            [ext drawInRect:CGRectMake(pad, hh * 0.88, tw, hh * 0.1) withFont:ef lineBreakMode:GMTailTruncation alignment:GMAlignCenter];
        }
    }
    // vệt sáng mép trên (kính iOS 6)
    [GBHex(0xFFFFFF, 0.10) setFill];
    CGContextFillRect(ctx, CGRectMake(0, 0, w, hh * 0.3));
    UIImage *img = UIGraphicsGetImageFromCurrentImageContext();
    UIGraphicsEndImageContext();
    if (img) [cache setObject:img forKey:ck];
    return img;
}

+ (UIImage *)frame:(UIImage *)img size:(CGSize)size {
    if (!img) return nil;
    UIGraphicsBeginImageContextWithOptions(size, NO, 0);
    CGRect r = CGRectMake(0, 0, size.width, size.height);
    [[UIBezierPath bezierPathWithRoundedRect:r cornerRadius:MAX(1.5, size.width * 0.03)] addClip];
    CGFloat s = MAX(size.width / img.size.width, size.height / img.size.height);
    CGSize d = CGSizeMake(img.size.width * s, img.size.height * s);
    [img drawInRect:CGRectMake((size.width - d.width) / 2, (size.height - d.height) / 2, d.width, d.height)];
    [GBHex(0x000000, 0.18) setStroke];
    UIBezierPath *edge = [UIBezierPath bezierPathWithRoundedRect:CGRectInset(r, 0.5, 0.5) cornerRadius:MAX(1.5, size.width * 0.03)];
    edge.lineWidth = 1;
    [edge stroke];
    UIImage *out = UIGraphicsGetImageFromCurrentImageContext();
    UIGraphicsEndImageContext();
    return out;
}

@end
