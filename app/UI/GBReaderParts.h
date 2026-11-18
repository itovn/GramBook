#import <UIKit/UIKit.h>

/// Một trang của UIPageViewController (lật cong): ảnh chụp trang của WebView (B5). Lật hai mặt: mỗi tờ có thêm mặt sau (`back`)
/// — giấy màu ngà (GBReaderPrefs backFor:), chữ của mặt trước in ngược, mờ, như giấy sách soi từ phía sau.
@interface GBPageVC : UIViewController
@property (nonatomic, strong) UIImage *image;
@property (nonatomic, strong) UIColor *paper;
/// Trang (màn) trong phần chương mà ảnh này chụp.
@property (nonatomic) NSInteger page;
@property (nonatomic, readonly) BOOL back;
+ (GBPageVC *)pageWithImage:(UIImage *)img page:(NSInteger)page paper:(UIColor *)paper;
/// Mặt sau của tờ `page` (`img` là ảnh mặt trước), `paper` là màu mặt sau.
+ (GBPageVC *)backWithImage:(UIImage *)img page:(NSInteger)page paper:(UIColor *)paper;
@end

/// Mục lục + dấu trang của sách đang đọc. `pick` nhận {s, frag} (mục lục) hoặc {pos} (dấu trang).
@interface GBTocViewController : UITableViewController
@property (nonatomic, copy) NSArray *toc;
@property (nonatomic, copy) NSArray *marks;
@property (nonatomic) NSInteger currentSpine;
@property (nonatomic, copy) void (^pick)(NSDictionary *target);
@property (nonatomic, copy) void (^removeMark)(NSDictionary *mark);
@end

/// Bảng «Aa»: độ sáng, cỡ chữ, font, nền, giãn dòng, lề, căn đều, độ đậm, kiểu lật trang, giữ kiểu của sách. Đổi gì gọi `changed`.
@interface GBStyleViewController : UITableViewController
@property (nonatomic, copy) void (^changed)(void);
/// Đổi độ sáng / ánh sáng ấm (chỉ cần tô lại lớp phủ — không dàn trang lại); nil thì gọi `changed`.
@property (nonatomic, copy) void (^lightChanged)(void);
@end

/// Cài đặt màn đọc (NSUserDefaults) dùng chung.
@interface GBReaderPrefs : NSObject
+ (NSArray *)fonts;           // [[tên hiện, họ font CSS (rỗng = theo sách), hệ số cỡ]]
+ (NSArray *)sizes;           // cỡ chữ px theo nấc
+ (NSInteger)sizeIndex;
+ (void)setSizeIndex:(NSInteger)i;
+ (NSInteger)fontIndex;
+ (void)setFontIndex:(NSInteger)i;
+ (NSString *)theme;          // day | sepia | gray | night
+ (void)setTheme:(NSString *)t;
+ (NSInteger)lineIndex;       // 0…2
+ (void)setLineIndex:(NSInteger)i;
+ (NSInteger)marginIndex;     // 0…2
+ (void)setMarginIndex:(NSInteger)i;
+ (BOOL)justify;
+ (void)setJustify:(BOOL)on;
+ (NSInteger)weight;          // 0…2
+ (void)setWeight:(NSInteger)w;
+ (NSString *)turn;           // curl | slide | none
+ (void)setTurn:(NSString *)t;
+ (BOOL)publisher;            // giữ kiểu của sách
+ (void)setPublisher:(BOOL)on;
+ (BOOL)twoPages;             // iPad ngang: 2 trang
+ (void)setTwoPages:(BOOL)on;
+ (CGFloat)dim;               // lớp tối thêm 0…0.6
+ (void)setDim:(CGFloat)d;
/// Độ sáng gộp −0,6…1: ≥ 0 = độ sáng màn hình; < 0 = màn hình tối nhất + lớp tối thêm (tối hơn mức thấp nhất của máy).
+ (CGFloat)light;
+ (void)setLight:(CGFloat)v;
/// Ánh sáng ấm 0…1: lớp màu hổ phách phủ màn đọc (lọc bớt ánh sáng xanh — iOS 5 / 6 không có Night Shift).
+ (CGFloat)warm;
+ (void)setWarm:(CGFloat)w;
+ (UIColor *)paperFor:(NSString *)theme;
+ (UIColor *)inkFor:(NSString *)theme;
/// Mặt sau tờ đang lật cong.
+ (UIColor *)backFor:(NSString *)theme;
@end
