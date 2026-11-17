/// 生成每个请求可以独立设置的选项相关代码的宏。目前只支持超时设置
#[macro_export]
macro_rules! add_per_request_options {
    ($type_name:ty) => {
        impl $type_name {
            /// 针对此次操作设置超时时间，单位为毫秒
            pub fn timeout_ms(mut self, timeout_ms: u64) -> Self {
                self.options.timeout_ms = Some(timeout_ms);
                self
            }
        }
    };
}
