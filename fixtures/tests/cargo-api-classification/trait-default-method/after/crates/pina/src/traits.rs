pub struct AccountInfo;
pub struct ProgramError;

pub trait ParseAccounts<'a>: Sized {
	fn parse_accounts(accounts: &'a [AccountInfo]) -> Result<Self, ProgramError>;

	#[doc(hidden)]
	fn __try_from_account_infos(accounts: &'a [AccountInfo]) -> Result<Self, ProgramError> {
		Self::parse_accounts(accounts)
	}
}
