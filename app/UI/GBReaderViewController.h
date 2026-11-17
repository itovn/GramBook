#import <UIKit/UIKit.h>

/// Màn đọc EPUB / MOBI / AZW3 (lõi chuyển sách Kindle bằng gb-mobi): UIWebView của chương (lõi phục vụ qua 127.0.0.1, reader.js chia cột — DECISIONS B7) phủ trên
/// UIPageViewController lật cong kiểu iBooks bằng ảnh chụp trang (B5). Chạm mép trái / phải = trang trước / sau, chạm giữa = hiện /
/// ẩn thanh, vuốt = lật cong theo ngón tay. Thanh trên: Thư viện · Mục lục · tên · Aa · Dấu trang; thanh dưới: thanh kéo trong chương.
@interface GBReaderViewController : UIViewController
/// `book`: dict sách (key bắt buộc; title, author… để hiện lúc đang mở).
- (instancetype)initWithBook:(NSDictionary *)book;
@property (nonatomic, readonly) NSString *key;
/// Đóng sách: lưu vị trí, báo lõi `book.close`, đóng bảng nổi (popover) rồi đóng màn đọc.
- (void)closeAnimated:(BOOL)animated;
/// App vừa đổi sáng / tối: đổi nền trang tại chỗ (giữ chỗ đang đọc).
- (void)themeChanged;
/// Nhớ / quên sách đang mở (mở lại khi app khởi động sau khi iOS tắt app lúc nằm nền).
+ (void)rememberOpen:(NSDictionary *)book;
/// Sách cần mở lại lúc app khởi động (nil: không có, hoặc lần mở lại trước làm app chết).
+ (NSDictionary *)bookToRestore;
/// Lệnh thử (GMDebug): next, prev, chapter <n>, info, toc, aa, chrome, close, turn <curl|slide|none>, dark <0|1> (gạt «Giao diện
/// tối» trong bảng Aa đang mở), night (nút trăng / mặt trời), warm <0…1> (ánh sáng ấm).
- (NSString *)debug:(NSString *)cmd arg:(NSString *)arg;
@end
