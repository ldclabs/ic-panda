# Política de Privacidade do dMsg

Última atualização: 2026-10-29.

O dMsg criptografa conteúdos privados nos seus dispositivos autorizados. Esta política abrange o site dmsg.net, o aplicativo web legado somente leitura, os aplicativos para desktop e iOS, o connect.dmsg.net, o serviço em nuvem e os canisters no Internet Computer (ICP). “Nós” refere-se à equipe que opera o dMsg sob a governança da ICPanda DAO. A disponibilidade de cada recurso depende da versão do aplicativo e da configuração do serviço.

Não vendemos dados pessoais nem utilizamos o seu conteúdo para publicidade. O site não contém rastreadores de anúncios ou de análise (analytics).

## 1. Conteúdo e informações públicas

Itens do cofre (Vault), mensagens privadas e arquivos são criptografados antes do envio. As chaves de conteúdo permanecem nos dispositivos autorizados ou em envelopes de recuperação/dispositivo que o serviço não consegue abrir. Os destinatários podem ler e manter cópias compartilhadas. Perfis públicos, avatares, handles (nomes de usuário), documentos de diretório e informações que você publica intencionalmente são públicos. Uma denúncia é uma divulgação separada: os moderadores recebem o motivo, a observação, os identificadores de conta/recurso e apenas o texto da mensagem que você escolher incluir explicitamente. Fazer uma denúncia não concede aos moderadores as chaves da sua conta.

## 2. Metadados do serviço

O serviço processa identificadores de conta e de login, chaves públicas de dispositivos, participação em canais, regras de contato e bloqueio, tamanhos de objetos, horários das solicitações, endereços IP e informações operacionais das requisições. O estado certificado da conta, os recibos de assinatura e os direitos de uso (entitlements) permitem que aplicativos clientes e serviços verifiquem a autoridade. Transações de ledger e registros on-chain públicos ou certificados geralmente não podem ser apagados.

Se você ativar as notificações, o serviço de notificações também processará o necessário para filtrar os alertas antes do envio: identificadores opacos de recursos, quais canais você silenciou ou ocultou, seu horário de silêncio e fuso horário, confirmações de visualização (quando você efetivamente viu um lembrete e até qual mensagem) e quais contas estão registradas na mesma instalação. Para lembretes de prazo de uma recuperação de conta pendente, seu dispositivo envia o registro de recuperação, incluindo a nova identidade de login e a chave pública do dispositivo; o serviço verifica esses dados e guarda apenas o lembrete e o horário de execução. O serviço não recebe nomes de canais, conteúdo de mensagens nem chaves para descriptografá-los. Revogar o registro, remover a conta do dispositivo ou excluir a conta do serviço apaga essas preferências, confirmações de visualização e vínculos de instalação; os registros de entrega e de nova tentativa são mantidos apenas até expirarem.

## 3. Serviços da Apple e pagamentos

No iOS, as notificações opcionais utilizam o serviço Apple Push Notification (APNs). A Apple e o nosso serviço de entrega processam um identificador de instalação, um token de dispositivo e metadados de entrega. As notificações contêm apenas um lembrete genérico e uma rota opaca, sem corpo da mensagem, senha ou nome de arquivo. Você pode desativar as notificações no dMsg ou nos ajustes do sistema; notificações já entregues nem sempre podem ser recolhidas. Se várias contas receberem notificações em uma mesma instalação, o token de dispositivo compartilhado permite que a Apple e o nosso serviço de entrega as associem; o dMsg pede sua permissão antes de ativar notificações para outra conta.

O App Attest processa um identificador de chave gerado pelo aplicativo, provas de atestado/asserção e desafios criptográficos para verificar a autenticidade da instalação do aplicativo. O Face ID, o Touch ID e o código de acesso do dispositivo são gerenciados pelo sistema operacional; o dMsg não recebe imagens biométricas nem a sua senha do dispositivo.

As compras via StoreKit são processadas pela Apple. Nosso serviço verificador e o canister de comércio processam identificadores de transação/produto, um token de vinculação à conta, datas de compra e expiração e o status de reembolso/revogação. Eles não recebem os dados do seu cartão de pagamento. Outros clientes do dMsg podem oferecer suporte a pagamentos via carteira ICP e associação PANDA, com registros públicos de ledger e de direitos. A exclusão da conta no dMsg não cancela uma assinatura da Apple; gerencie suas assinaturas na sua Conta Apple.

As políticas e propostas de carteiras multifirma (multisig) são processadas pelos canisters da carteira, e sua visibilidade segue as configurações da própria carteira. Provedores de RPC de blockchain recebem os endereços, dados de transação e informações de IP necessários para consultas de saldo, simulação de taxas e transmissão de transações (broadcasting). Os endpoints padrão incluem mempool.space, PublicNode, mainnet.base.org e api.mainnet-beta.solana.com; endpoints alternativos configurados por você possuem suas próprias práticas de privacidade.

## 4. Dados locais e integrações do sistema

Os aplicativos nativos mantêm um espaço de trabalho local criptografado, chaves protegidas pelo sistema operacional (onde suportado), rascunhos, operações pendentes e preferências. O desbloqueio disponibiliza o conteúdo selecionado na memória do aplicativo. Arquivos exportados e textos copiados podem ser retidos pelos aplicativos de destino.

A extensão Compartilhar (Share) do iOS apenas prepara os itens selecionados por você para uma importação posterior na conta. As integrações com o Preenchimento Automático de Senhas (AutoFill) e com o app Arquivos contêm apenas instantâneos (snapshots) selecionados explicitamente e criptografados separadamente. O Preenchimento Automático exige autenticação independente do sistema; os instantâneos expiram após 24 horas. As sessões de acesso ao app Arquivos duram no máximo cinco minutos e terminam quando o dMsg é bloqueado, mas o app Arquivos e outros destinatários podem manter cópias já baixadas. Alterar os itens de origem não atualiza automaticamente um instantâneo exportado.

Transferências explícitas em segundo plano armazenam apenas texto cifrado limitado e autorização de transferência em um diretório separado do dispositivo, acessível após o primeiro desbloqueio do aparelho. Elas não guardam chaves de assinatura da conta nem chaves de conteúdo. A conta original precisa ser desbloqueada para verificar e concluir a transferência. O sistema operacional pode atrasar ou interromper a entrega.

O site lembra o seu idioma localmente. O aplicativo web legado utiliza o armazenamento do navegador para sessões e dados em cache; a visualização de carteira dele pode consultar preços no ICPSwap. Desinstalar um aplicativo ou limpar seus dados pode causar a perda permanente de conteúdos que nunca foram sincronizados ou exportados.

## 5. Provedores e retenção de dados

A Cloudflare processa o texto cifrado na nuvem e os metadados de requisição. Provedores de nós e gateways do ICP operam os canisters e sites. A Apple distribui o aplicativo para iOS e fornece os serviços opcionais descritos acima. O Internet Identity, provedores de carteiras e aplicativos conectados possuem suas próprias políticas de privacidade. Podemos divulgar informações que detemos quando exigido por lei, incluindo denúncias enviadas explicitamente; não podemos revelar textos em claro (plaintext) que não possuímos.

O texto cifrado na nuvem segue as regras de armazenamento e exclusão da conta. A exclusão da conta no serviço é uma solicitação de limpeza acompanhada: ela bloqueia novos acessos ao serviço, remove conteúdos na nuvem pertencentes à conta e cópias públicas de avatar, e só informa a conclusão após a limpeza. Ela não apaga cópias compartilhadas com outras pessoas, cópias locais em outros dispositivos, o histórico público do ledger ou registros financeiros e anti-replay necessários. Transações pendentes ou canais ativos de sua propriedade podem precisar ser resolvidos antes. Registros de denúncias são retidos por até 90 dias para análise; tokens de entrega expiram caso não sejam renovados. Registros operacionais (logs) são mantidos apenas pelo tempo necessário para segurança, prevenção de abusos e solução de problemas.

## 6. Escolhas, direitos e contato

Você pode exportar ou excluir conteúdos, revogar dispositivos ou navegadores, bloquear remetentes, denunciar abusos, alterar regras de contato, solicitar a exclusão da conta no serviço e remover separadamente os dados da conta deste dispositivo. Dependendo da sua localização, você pode ter o direito de acessar, corrigir, excluir ou se opor ao processamento de informações pessoais. Entre em contato com o [Suporte](https://dmsg.net/_/support) para solicitações ou relatos privados de vulnerabilidades. Explicaremos quando a criptografia, registros de transações retidos ou registros públicos imutáveis limitarem o atendimento de uma solicitação.

Nenhum sistema é perfeitamente seguro. Atualizações de aplicativos e upgrades de canisters sob a governança da DAO continuam sendo pontos de confiança. O dMsg não é destinado a menores de 13 anos ou abaixo da idade mínima aplicável para uso de serviços online sem consentimento dos pais ou responsáveis. Atualizamos a data no topo desta página e anunciamos alterações relevantes na política pelo aplicativo ou no site.
