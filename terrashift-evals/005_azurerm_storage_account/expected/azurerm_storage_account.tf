resource "azurerm_storage_account" "assets" {
  name = "myorgassets"
  resource_group_name = azurerm_resource_group.main.name
  location = "East US"
  account_tier = "Standard"
  account_replication_type = "LRS"
}
