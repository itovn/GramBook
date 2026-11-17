#import <UIKit/UIKit.h>

/// Nguồn sách: các chat có sách tìm được khi quét tài khoản (Tin nhắn đã lưu đầu tiên, rồi theo số cuốn) — công tắc ẩn / hiện
/// trong Thư viện, chạm để xem sách của nguồn; «+» thêm kênh / nhóm công khai bằng @tên hoặc link t.me (kể cả chưa vào kênh);
/// đầu bảng: trạng thái quét, «Lấy sách mới», «Quét toàn bộ». Mục «Sách trên máy» duyệt thư mục Documents.
@interface GMSourcesViewController : UITableViewController
@end

/// Chủ đề của nhóm diễn đàn: «Tất cả sách của nhóm», rồi từng chủ đề có sách (số cuốn) — chạm để xem riêng chủ đề; nút «Mỗi chủ đề
/// thành một bộ sưu tập» tạo nhanh bộ «Nhóm · Chủ đề» cho mọi chủ đề.
@interface GMTopicsViewController : UITableViewController
@property (nonatomic, copy) NSString *peer;
@end
