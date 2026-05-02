resource "azurerm_virtual_network" "main" {
  name = "main"
  resource_group_name = azurerm_resource_group.main.name
  location = "East US"
}
