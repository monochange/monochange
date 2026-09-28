module github.com/acme/service

go 1.22

require (
	github.com/acme/core v1.2.0
	github.com/other/core v0.9.0
	github.com/acme/sdk/v2 v2.0.0
	github.com/other/sdk/v2 v0.1.0
)

replace github.com/acme/core => ../core
