#import <UIKit/UIKit.h>

/// Sách trên máy: duyệt thư mục (mặc định ~/Documents — chép sách vào bằng iFile / SSH / iTunes), chạm tệp EPUB / MOBI / AZW3…
/// để đọc (khoá `file:<đường dẫn>` — lõi đọc thẳng tệp, không cần đăng nhập Telegram).
@interface GBLocalViewController : UITableViewController
- (instancetype)initWithPath:(NSString *)path;
@end
