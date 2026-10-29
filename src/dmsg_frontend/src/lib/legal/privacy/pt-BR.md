# dMsg Política de Privacidade

Última atualização: 29/10/2026.

dMsg criptografa conteúdo privado em seus dispositivos autorizados. Esta política cobre dmsg.net, o aplicativo web legado somente leitura, os aplicativos para desktop e iOS, connect.dmsg.net, o serviço de nuvem e o Computador de Internet (ICP) canisters. “Nós” significa a equipe que opera dMsg sob a governança ICPanda DAO. A disponibilidade de recursos individuais depende da versão do aplicativo e da configuração do serviço.

Não vendemos dados pessoais nem usamos seu conteúdo para publicidade. O site não contém publicidade ou rastreadores analíticos.

## 1. Conteúdo e informações públicas

Itens do Vault, mensagens privadas e arquivos são criptografados antes do upload. As chaves de conteúdo permanecem em dispositivos autorizados ou em envelopes de recuperação/dispositivos que o serviço não consegue abrir. Os destinatários podem ler e reter cópias compartilhadas. Perfis públicos, avatares, identificadores, documentos de diretório e informações que você publica intencionalmente são públicos. Um relatório é uma divulgação separada: os moderadores recebem seu motivo, nota, identificadores de conta/recurso e apenas o texto da mensagem que você explicitamente escolhe incluir. Os relatórios não fornecem aos moderadores as chaves da sua conta.

## 2. Metadados de serviço

O serviço processa identificadores de conta e login, chaves públicas de dispositivos, associação de canal, regras de contato e bloqueio, tamanhos de objetos, tempo de solicitação, endereços IP e informações de solicitação operacional. O estado da conta certificada, os recibos de assinatura e os direitos permitem que clientes e serviços verifiquem a autoridade. As transações contábeis e os registros públicos ou certificados na rede geralmente não podem ser apagados.

Se você ativar as notificações, o serviço de notificações também processa o necessário para filtrar os alertas antes do envio: identificadores opacos de recursos, quais canais você silenciou ou ocultou, seu horário de silêncio e fuso horário, confirmações de visualização (quando você realmente viu um lembrete e até qual mensagem) e quais contas estão registradas na mesma instalação. Para lembretes de prazo de uma recuperação de conta pendente, seu dispositivo envia o registro de recuperação, incluindo a nova identidade de login e a chave pública do dispositivo; o serviço o verifica e guarda apenas o lembrete e o horário de execução. O serviço não recebe nomes de canais, conteúdo de mensagens nem chaves para descriptografá-los. Revogar o registro, remover a conta do dispositivo ou excluir a conta do serviço apaga essas preferências, confirmações de visualização e vínculos de instalação; os registros de entrega e de nova tentativa são mantidos apenas até expirarem.

## 3. Serviços e pagamentos da Apple

No iOS, as notificações opcionais usam o serviço Apple Push Notification. A Apple e o nosso serviço de entrega processam um identificador de instalação, um token de dispositivo e metadados de entrega. As notificações contêm um lembrete genérico e uma rota opaca, não um corpo de mensagem, senha ou nome de arquivo. Você pode desativar notificações em dMsg ou nas configurações do sistema; as notificações já entregues nem sempre podem ser recuperadas. Se várias contas recebem notificações em uma mesma instalação, o token de dispositivo compartilhado permite que a Apple e o nosso serviço de entrega as relacionem; o dMsg pede sua permissão antes de ativar notificações para outra conta.

O App Attest processa um identificador de chave gerado pelo aplicativo, evidências de atestado/declaração e desafios para verificar a instalação de um aplicativo. Face ID, Touch ID e a senha do dispositivo são gerenciados pelo sistema operacional; dMsg não recebe imagens biométricas ou sua senha.

As compras do StoreKit são feitas pela Apple. Nosso verificador e comércio canister processa identificadores de transação/produto, um token de vinculação de conta, datas de compra e vencimento e status de reembolso/revogação. Eles não recebem os detalhes do seu cartão de pagamento. Outros clientes dMsg podem oferecer suporte a pagamentos de carteira ICP e associação PANDA, com razão pública e registros de direitos. A exclusão da conta não cancela uma assinatura da Apple; gerenciar assinaturas em sua conta Apple.

As políticas e propostas da carteira da equipe são processadas pela carteira canisters; sua visibilidade segue as configurações da carteira. Os provedores Blockchain RPC recebem os endereços, dados de transação e informações de IP necessários para consultas de saldo, simulação de taxas e transmissão. Os endpoints padrão incluem mempool.space, PublicNode, mainnet.base.org e api.mainnet-beta.solana.com; as alternativas configuradas têm suas próprias práticas de privacidade.

## 4. Dados locais e integrações de sistemas

Os aplicativos nativos mantêm um espaço de trabalho local criptografado, chaves protegidas pelo sistema operacional onde são suportadas, rascunhos, operações pendentes e preferências. O desbloqueio disponibiliza o conteúdo selecionado na memória do aplicativo. Os arquivos exportados e o texto copiado podem ser retidos pelos aplicativos de destino.

A extensão iOS Share prepara apenas os itens que você seleciona para uma importação posterior da conta. As integrações de preenchimento automático de senha e arquivos contêm apenas instantâneos criptografados separadamente selecionados explicitamente. O AutoFill requer autenticação de sistema independente; os instantâneos expiram após 24 horas. As sessões de acesso aos arquivos duram no máximo cinco minutos e terminam quando o dMsg é bloqueado, mas o aplicativo Arquivos e outros destinatários podem reter cópias já baixadas. A alteração de itens de origem não atualiza automaticamente um instantâneo exportado.

As transferências explícitas em segundo plano armazenam apenas texto cifrado limitado e autoridade de transferência em um diretório de dispositivo separado, acessível após o primeiro desbloqueio do dispositivo. Eles não possuem assinatura de conta ou chaves de conteúdo. A conta original deve ser desbloqueada para verificar e concluir uma transferência. O sistema pode atrasar ou interromper a entrega.

O site lembra seu idioma localmente. O aplicativo legado usa armazenamento do navegador para sessões e dados armazenados em cache; sua visualização de carteira pode solicitar preços do ICPSwap. Remover um aplicativo ou limpar seus dados pode causar a perda permanente de conteúdo que nunca foi sincronizado ou exportado.

## 5. Provedores e retenção

Cloudflare processa texto cifrado em nuvem e solicita metadados. Provedores de nós e gateways ICP operam canisters e sites. A Apple distribui o aplicativo iOS e fornece os serviços opcionais descritos acima. Internet Identity, provedores de carteira e aplicativos conectados têm suas próprias políticas de privacidade. Poderemos divulgar informações que possuímos quando exigido por lei, incluindo relatórios enviados explicitamente; não podemos divulgar texto simples que não possuímos.

O texto cifrado na nuvem segue as regras de armazenamento e exclusão da conta. A exclusão da conta de serviço é uma solicitação de limpeza rastreada: ela bloqueia o acesso a novos serviços, remove o conteúdo da nuvem de propriedade da conta e cópias públicas de avatares e relata a conclusão somente após a limpeza. Ele não apaga cópias compartilhadas de outras pessoas, cópias locais em outros dispositivos, histórico contábil público ou registros financeiros/anti-repetição necessários. Transações não liquidadas ou canais ativos próprios podem precisar de resolução primeiro. Os registros do relatório são retidos por até 90 dias para revisão; os tokens de entrega expiram, a menos que sejam renovados. Os logs operacionais são mantidos apenas quando necessário para segurança, prevenção de abusos e solução de problemas.

## 6. Escolhas, direitos e contato

Você pode exportar ou excluir conteúdo, revogar dispositivos ou navegadores, bloquear remetentes, denunciar abusos, alterar regras de contato, solicitar a exclusão de contas de serviço e remover separadamente os dados da conta deste dispositivo. Dependendo da sua localização, você pode ter direitos de acesso, correção, exclusão ou oposição ao processamento de informações pessoais. Entre em contato com o [Suporte](https://dmsg.net/_/support) para solicitações ou relatórios de vulnerabilidade privados. Explicamos quando a criptografia, os registros de transações retidos ou os registros públicos imutáveis ​​limitam uma solicitação.

Nenhum sistema é perfeitamente seguro. As atualizações de aplicativos e canister sob a governança DAO permanecem pontos de confiança. dMsg não se destina a crianças menores de 13 anos ou abaixo da idade mínima aplicável para utilização de serviços online sem o consentimento dos pais. Atualizamos a data acima e anunciamos alterações materiais na política no aplicativo ou no site.
