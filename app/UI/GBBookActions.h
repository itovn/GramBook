#import <UIKit/UIKit.h>

/// Hỏi một dòng chữ (UIAlertView có ô nhập): Huỷ / Lưu. Chép từ GramMusic.
void GMAskText(NSString *title, NSString *placeholder, NSString *initial, void (^done)(NSString *text));

/// Bảng thao tác «⋯» của một cuốn: Đọc, Tải về máy / Xoá bản tải, Yêu thích, Muốn đọc, Thêm vào bộ sưu tập…, Gửi cho bạn qua
/// Telegram…, Đánh dấu đã đọc xong, (trong bộ sưu tập) Bỏ khỏi bộ này. `changed` nhận thông tin sách mới (dl, fav…); `removed` khi sách bị bỏ khỏi danh sách.
@interface GBBookActions : NSObject
+ (void)showFor:(NSDictionary *)book in:(UIViewController *)vc from:(UIView *)anchor collection:(id)collectionId
        changed:(void (^)(NSDictionary *book))changed removed:(void (^)(void))removed;
/// Gửi sách cho bạn: chọn chat rồi chuyển tiếp tin gốc chứa tệp (sách «hạn chế lưu nội dung» không gửi được).
+ (void)send:(NSDictionary *)book from:(UIViewController *)vc;
/// Chọn bộ sưu tập để thêm các sách (hoặc tạo bộ mới).
+ (void)addKeys:(NSArray *)keys toCollectionFrom:(UIViewController *)vc;
@end
