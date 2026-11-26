# Signs Microsoft's Store Developer CLI (msstore) in to Partner Center. Used
# by .github\workflows\store.yml, which passes the four values in through the
# environment so that they never appear on a command line in the build log.

$ErrorActionPreference = "Stop"

foreach ($name in "TENANT_ID", "SELLER_ID", "CLIENT_ID", "CLIENT_SECRET") {
    if (-not [Environment]::GetEnvironmentVariable($name)) { throw "The PARTNER_CENTER_$name secret is not set." }
}
# The CLI reads this one as a number and fails obscurely on anything else.
if ($env:SELLER_ID -notmatch '^\s*\d+\s*$') {
    throw "The PARTNER_CENTER_SELLER_ID secret has to be the Seller ID, which is digits only (Partner Center > Account settings > Legal info)."
}

$seller = $env:SELLER_ID.Trim()
msstore reconfigure --tenantId $env:TENANT_ID --sellerId $seller --clientId $env:CLIENT_ID --clientSecret $env:CLIENT_SECRET
if ($LASTEXITCODE -ne 0) { throw "Partner Center did not accept the credentials." }
