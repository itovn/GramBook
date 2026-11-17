#import <UIKit/UIKit.h>

/// Bìa tạm khi sách không có bìa (vẽ tay, CoreGraphics — cả iOS 5 / 6): khổ sách 2 : 3, dải màu theo khoá sách, gáy tối bên
/// trái, tên sách + tác giả chữ trắng, nhãn định dạng (EPUB / MOBI…) ở chân. Có bộ đệm theo (khoá, cỡ).
@interface GBCover : NSObject
+ (UIImage *)placeholderFor:(NSDictionary *)book size:(CGSize)size;
/// Ảnh bìa thật thu vào khung `size` (giữ tỉ lệ, cắt phần thừa, bo góc nhẹ, viền mờ).
+ (UIImage *)frame:(UIImage *)img size:(CGSize)size;
@end
