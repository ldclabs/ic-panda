# dmsg_wallet

占位 canister：为 [icp.yaml](../../icp.yaml) 中登记的 `dmsg_wallet` 保留主网 ID `ghcje-dyaaa-aaaas-qg7va-cai`，使不带名称的 `icp build` / `icp deploy` 能构建全部 canister。它没有方法（[dmsg_wallet.did](dmsg_wallet.did) 为空服务），不保存状态，也不在 canisters release 中发布。钱包的设计尚未确定，实现时替换本 crate。
