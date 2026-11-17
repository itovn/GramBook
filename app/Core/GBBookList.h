#import <Foundation/Foundation.h>

/// Danh sách sách lấy từng trang 100 cuốn từ lõi (`library.query`) — kênh sách có thể hàng chục nghìn tệp, máy A4 chỉ 256 MB RAM.
/// Bảng hỏi `bookAt:`; trang chưa có thì trả nil và tự tải, tải xong gọi `changed`. Chép từ GMSongList của GramMusic.
@interface GBBookList : NSObject
/// Từ cần tìm (bỏ dấu ở lõi), nguồn (khoá peer, nil = mọi nguồn), cách xếp (date | title | author), lọc (all | offline | cached |
/// readable | unread | reading | done), nhóm định dạng (nil | epub | kindle | pdf | text | other).
@property (nonatomic, copy) NSString *query;
@property (nonatomic, copy) NSString *source;
@property (nonatomic, copy) NSString *sort;
@property (nonatomic, copy) NSString *filter;
@property (nonatomic, copy) NSString *fmt;
/// Chủ đề của nhóm diễn đàn (nil = mọi sách; 1 = General).
@property (nonatomic, strong) NSNumber *topic;
@property (nonatomic, readonly) NSInteger total;
/// Mục chữ cái (xếp theo tên / tác giả): [{t: "A", i: vị trí đầu, n: số sách}]; rỗng khi xếp theo ngày.
@property (nonatomic, readonly) NSArray *sections;
@property (nonatomic, readonly) BOOL loaded;
@property (nonatomic, copy) void (^changed)(void);

- (void)reload;
- (NSDictionary *)bookAt:(NSInteger)i;
/// Thay thông tin một sách đã có trong trang (Yêu thích, đã tải…).
- (void)replaceBook:(NSDictionary *)book;
/// Thông tin các sách đã có trong trang (khoá → dict).
- (NSDictionary *)knownInfo;
@end
