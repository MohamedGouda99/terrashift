resource "azurerm_linux_virtual_machine" "web" {
  name = "web-vm"
  resource_group_name = azurerm_resource_group.main.name
  location = "East US"
  size = "Standard_D2s_v3"
  admin_username = "azureuser"
}
