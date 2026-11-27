pub struct AccountInfo;
pub struct ProgramError;

pub trait ParseAccounts<'a>: Sized {
	fn parse_accounts(accounts: &'a [AccountInfo]) -> Result<Self, ProgramError>;
}
