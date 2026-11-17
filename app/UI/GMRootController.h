#import <UIKit/UIKit.h>

@class GBReaderViewController;

/// Khung chính: thanh dưới (Đang đọc / Thư viện / Nguồn / Bộ sưu tập / Cài đặt), thanh «Đọc tiếp» nằm ngay trên thanh dưới (sách đọc
/// dở gần nhất — chạm là đọc tiếp đúng trang). Tự làm thay UITabBarController (iOS 5 có API chứa màn con) để đặt được thanh «Đọc
/// tiếp» giữa nội dung và thanh dưới. Chép từ GramMusic (thanh phát nhỏ → thanh «Đọc tiếp»).
@interface GMRootController : UIViewController <UITabBarDelegate>
@property (nonatomic) NSInteger selectedIndex;
/// Tài khoản đang đăng nhập (nil = chưa).
@property (nonatomic, readonly) NSDictionary *me;
/// Màn đọc đang mở (nil = không).
@property (nonatomic, readonly) GBReaderViewController *reader;
- (void)loggedIn:(NSDictionary *)me;
- (void)loggedOut;
/// Mở màn đọc cho sách `book` (dict có key).
- (void)openBook:(NSDictionary *)book;
@end

/// Khung chính đang dùng.
GMRootController *GMRoot(void);
