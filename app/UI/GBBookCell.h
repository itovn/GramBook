#import <UIKit/UIKit.h>

/// Ô sách: bìa 2 : 3 (sách không có bìa: bìa tạm vẽ tay), tên, dòng phụ «tác giả · EPUB · 1,2 MB · nguồn», vạch tiến độ đọc, nhãn
/// đỏ cho định dạng chưa đọc được, dấu đã tải, nút «⋯».
/// Định dạng màn đọc mở được (khớp `text::readable_ext` của lõi): EPUB + Kindle (MOBI / AZW3 / AZW / PRC).
BOOL GBReadableExt(NSString *ext);

@interface GBBookCell : UITableViewCell
@property (nonatomic, copy) void (^more)(void);
/// `progress`: 0…1 đang tải về máy, < 0 = không tải. `showSource`: ghi tên nguồn ở dòng phụ.
- (void)setBook:(NSDictionary *)book progress:(double)progress showSource:(BOOL)showSource;
- (void)setLoading;
+ (CGFloat)height;
@end
