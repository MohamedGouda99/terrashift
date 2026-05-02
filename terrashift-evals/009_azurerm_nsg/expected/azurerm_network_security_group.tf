resource "azurerm_network_security_group" "web" {
  name = "allow-web-nsg"
  location = "East US"
  resource_group_name = azurerm_resource_group.main.name
}
