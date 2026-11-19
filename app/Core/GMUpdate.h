#import <Foundation/Foundation.h>

/// Sắp cài bản cập nhật (gbupdate sẽ đóng app): màn đọc lưu vị trí ngay.
extern NSString *const GMUpdateWillInstallNotification;

/// Cập nhật OTA (chép từ GramMini — D33 của GramMini; DECISIONS B16): đọc bản mới nhất trên GitHub Releases (địa chỉ API ở
/// Info.plist `GMUpdateURL`; bản thử đổi được bằng NSUserDefaults `update.api`), tìm tệp `vn.grambook.app_<bản>_iphoneos-arm.deb`
/// + chữ ký `<tệp>.sig`. Kiểm lúc mở app (sau 10 giây) và khi quay lại app (tối đa 6 giờ một lần), hỏi trước khi cập nhật
/// («Để sau» = không hỏi lại bản đó trong 24 giờ); tải qua lõi (`http.download` — rustls, iOS 5 / 6 không tự vào được GitHub),
/// kiểm kích thước + SHA-256 (`digest` của GitHub), rồi gọi /usr/libexec/grambook/gbupdate (root, setuid): kiểm chữ ký → dpkg -i →
/// khởi động lại SpringBoard → mở lại app.
@interface GMUpdate : NSObject
/// Gọi một lần lúc mở app.
+ (void)start;
/// Cài đặt → Phiên bản: kiểm ngay, báo cả khi đang dùng bản mới nhất.
+ (void)checkManual;
#ifdef DEBUG
/// Lệnh thử «otalocal»: cài gói + chữ ký (`<gói>.sig`) đã chép sẵn vào Documents/GramBook-update — thử gbupdate không cần GitHub.
+ (void)debugInstall:(NSString *)deb;
#endif
@end
