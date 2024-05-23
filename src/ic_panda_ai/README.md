# `ic_panda_ai`
`ic_panda_ai` is an DeAI app, an LLM chatbot, named Panda Oracle, running in an ICP canister.

## On-chain canister

https://dashboard.internetcomputer.org/canister/bwwuq-byaaa-aaaan-qmk4q-cai

## Running locally

Deploy to local network:
```bash
dfx canister create --specified-id bwwuq-byaaa-aaaan-qmk4q-cai ic_panda_ai
dfx deploy ic_panda_ai

# install upload tool
cargo install --bin ic-oss-cli --git https://github.com/ldclabs/ic-oss
# create a identity for uploading model files
ic-oss-cli identity --new --file myid.pem
# Output:
# principal: lxph3-nvpsv-yrevd-im4ug-qywcl-5ir34-rpsbs-6olvf-qtugo-iy5ai-jqe
# new identity: myid.pem

# add the identity as a manager
dfx canister call bwwuq-byaaa-aaaan-qmk4q-cai admin_set_managers '(vec {principal "lxph3-nvpsv-yrevd-im4ug-qywcl-5ir34-rpsbs-6olvf-qtugo-iy5ai-jqe"})'

# download model files from https://huggingface.co/Qwen/Qwen1.5-0.5B-Chat
git clone https://huggingface.co/Qwen/Qwen1.5-0.5B-Chat

# upload model files
ic-oss-cli -i myid.pem upload -b bwwuq-byaaa-aaaan-qmk4q-cai --file Qwen1.5-0.5B-Chat/config.json
# ... file id: 1 ...

ic-oss-cli -i myid.pem upload -b bwwuq-byaaa-aaaan-qmk4q-cai --file Qwen1.5-0.5B-Chat/tokenizer.json
# ... file id: 2 ...

ic-oss-cli -i myid.pem upload -b bwwuq-byaaa-aaaan-qmk4q-cai --file Qwen1.5-0.5B-Chat/model.safetensors
# ... file id: 3 ...

# set model files
dfx canister call bwwuq-byaaa-aaaan-qmk4q-cai admin_load_model '(record {config_id=1;tokenizer_id=2;model_id=3})'
# Output:
# (
#   variant {
#     Ok = record {
#       total_instructions = 19_720_034_211 : nat64;
#       load_mode_instructions = 0 : nat64;
#       load_file_instructions = 19_720_033_979 : nat64;
#     }
#   },
# )

# chat with the AI
dfx canister call bwwuq-byaaa-aaaan-qmk4q-cai update_chat '(record {prompt="Nice to chat with you, Please introduce yourself."})'
# Output:
# (
#   variant {
#     Ok = record {
#       instructions = 1_128_692_812_645 : nat64;
#       tokens = 43 : nat32;
#       message = "\nHello! I am Panda Oracle, a giant panda with human intelligence. I am here to help you with any questions or concerns you may have. How can I assist you today?\n";
#     }
#   },
# )
```

## License
Copyright © 2024 [LDC Labs](https://github.com/ldclabs).

`ldclabs/ic-panda` is licensed under the MIT License. See [LICENSE](LICENSE) for the full license text.